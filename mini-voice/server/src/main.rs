mod config;
use anyhow::Result;
use config::{Action, Config};
use futures_util::{SinkExt, StreamExt};
use mini_server::{now, Out, Server};
use mini_shared::*;
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    net::{TcpListener, UdpSocket},
    sync::{mpsc, Semaphore},
};
use tokio_tungstenite::{
    accept_async_with_config,
    tungstenite::{protocol::WebSocketConfig, Message},
};
fn main() -> Result<()> {
    let action = config::arguments()?;
    let (path, check) = match action {
        Action::Help => {
            println!("mini-server [server.json]\nmini-server --check-config server.json\nmini-server --version\n\nTLS is mandatory. Relative file paths use the current working directory.");
            return Ok(());
        }
        Action::Version => {
            println!("mini-server {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Action::Run(p) => (p, false),
        Action::Check(p) => (p, true),
    };
    let (c, tls) = Config::load(&path)?;
    if check {
        println!("Configuration and TLS key pair are valid; no sockets or database opened.");
        return Ok(());
    }
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(4)
        .enable_all()
        .build()?
        .block_on(run(c, tls))
}
async fn shutdown_signal() -> Result<()> {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {result=tokio::signal::ctrl_c()=>result?,_=terminate.recv()=>{}}
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await?;
    Ok(())
}
async fn run(c: Config, tls: tokio_rustls::rustls::ServerConfig) -> Result<()> {
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(tls));
    let udp = Arc::new(UdpSocket::bind(&c.udp_listen).await?);
    let state = Arc::new(Mutex::new(Server::open(
        &c.database,
        udp.local_addr()?.port(),
    )?));
    {
        let s = state.clone();
        let udp = udp.clone();
        tokio::spawn(async move {
            let mut buf = [0u8; 1400];
            loop {
                let Ok((n, addr)) = udp.recv_from(&mut buf).await else {
                    continue;
                };
                if n < 40 {
                    continue;
                }
                let packets = {
                    let mut s = s.lock().unwrap();
                    let uid = s
                        .peers
                        .iter()
                        .find(|(_, p)| p.generation == buf[..16])
                        .map(|(&id, _)| id);
                    if let Some(u) = uid {
                        let p = s.peers.get_mut(&u).unwrap();
                        if p.expiry <= now() {
                            continue;
                        }
                        if let Some(body) = p.up.open(&buf[..n]) {
                            p.addr = Some((addr, now()));
                            if body.is_empty() {
                                p.down
                                    .seal(&[])
                                    .map(|b| vec![(addr, b)])
                                    .unwrap_or_default()
                            } else {
                                s.relay(u, &body)
                            }
                        } else {
                            vec![]
                        }
                    } else {
                        vec![]
                    }
                };
                for (a, b) in packets {
                    let _ = udp.send_to(&b, a).await;
                }
            }
        });
    }
    let listener = TcpListener::bind(&c.listen).await?;
    eprintln!(
        "TLS signaling {} / UDP {}",
        listener.local_addr()?,
        udp.local_addr()?
    );
    let connections = Arc::new(Semaphore::new(128));
    let auth_slots = Arc::new(Semaphore::new(2));
    let attempts = Arc::new(Mutex::new(
        HashMap::<std::net::IpAddr, (Instant, u32)>::new(),
    ));
    loop {
        tokio::select! {result=shutdown_signal()=>{result?;break;},accept=listener.accept()=>{let(tcp,addr)=accept?;let Ok(permit)=connections.clone().try_acquire_owned() else{continue;};
        let allowed={let mut a=attempts.lock().unwrap();a.retain(|_,v|v.0.elapsed()<Duration::from_secs(60));let v=a.entry(addr.ip()).or_insert((Instant::now(),0));v.1+=1;v.1<=20};if !allowed{continue;}
        let acceptor=acceptor.clone();let s=state.clone();let udp=udp.clone();let slots=auth_slots.clone();tokio::spawn(async move{let _permit=permit;let result:Result<()>=async{
        let tls=tokio::time::timeout(Duration::from_secs(10),acceptor.accept(tcp)).await??;
        let config=WebSocketConfig::default().max_message_size(Some(MAX_FRAME)).max_frame_size(Some(MAX_FRAME));
        let ws=tokio::time::timeout(Duration::from_secs(10),accept_async_with_config(tls,Some(config))).await??;
        serve(ws,s,udp,slots,addr).await}.await;if let Err(e)=result{eprintln!("Connection {addr}: {e}");}});
        }}
    }
    let shutdown_state = state.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let mut state = shutdown_state.lock().unwrap();
        for peer in state.peers.values() {
            let _ = peer.tx.try_send(Out::Close);
        }
        state.rooms.clear();
        state.db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
    })
    .await??;
    eprintln!("Server stopped; SQLite checkpoint complete");
    Ok(())
}
async fn serve<S>(
    ws: tokio_tungstenite::WebSocketStream<S>,
    state: Arc<Mutex<Server>>,
    udp: Arc<UdpSocket>,
    slots: Arc<Semaphore>,
    _addr: SocketAddr,
) -> Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (mut sink, mut stream) = ws.split();
    let (tx, mut rx) = mpsc::channel::<Out>(128);
    let mut identity = None;
    let mut heartbeat = tokio::time::interval(Duration::from_secs(10));
    let mut last = Instant::now();
    let mut tick = Instant::now();
    let mut requests = 0;
    let mut auth_attempts = 0;
    let result:Result<()>=async{loop{tokio::select!{
 _=heartbeat.tick()=>{if last.elapsed()>Duration::from_secs(35){break;}
 if let Some((u,g))=identity{let mut s=state.lock().unwrap();s.expire_rejoins();if !s.peers.get(&u).is_some_and(|p|p.generation==g&&p.expiry>now()&&!p.overloaded.load(std::sync::atomic::Ordering::Relaxed))||tx.capacity()==0{break;}}send_frame(&mut sink,Message::Ping(vec![].into())).await?;},
 out=rx.recv()=>{match out{Some(Out::Json(e))=>send_frame(&mut sink,Message::Text(serde_json::to_string(&e)?.into())).await?,Some(Out::Audio(b))=>send_frame(&mut sink,Message::Binary(b.into())).await?,Some(Out::Replaced)=>{send_frame(&mut sink,Message::Text(serde_json::to_string(&Event::Error{message:"Signed in elsewhere".into()})?.into())).await?;break;},_=>break,}},
 incoming=stream.next()=>{let Some(incoming)=incoming else{break;};let msg=incoming?;last=Instant::now();match msg {
 Message::Text(text)=>{
 if tick.elapsed()>=Duration::from_secs(60){tick=Instant::now();requests=0;}requests+=1;if requests>120{break;}
 let req=match serde_json::from_str::<Request>(&text){Ok(r)=>r,Err(_)=>{send_frame(&mut sink,Message::Text(serde_json::to_string(&Event::Error{message:"Malformed request".into()})?.into())).await?;continue;}};
 let s=state.clone();let tx2=tx.clone();let auth=identity.is_none();if auth{auth_attempts+=1;if auth_attempts>5{break;}}
 let failed_nonce=match &req{Request::Send{nonce,..}=>Some(nonce.clone()),_=>None};
 let logged_out=matches!(req,Request::Logout {});
 let permit=if auth{Some(slots.clone().acquire_owned().await?)}else{None};let who=identity;
 let r=tokio::task::spawn_blocking(move||{let _permit=permit;if let Some((u,g))=who{let mut s=s.lock().unwrap();anyhow::ensure!(s.peers.get(&u).is_some_and(|p|p.generation==g),"Session replaced");s.handle(u,req)?;Ok(None)}else{let work=s.lock().unwrap().auth_work(req)?;let proof=work.verify()?;s.lock().unwrap().auth_verified(proof,tx2).map(Some)}}).await?;
 match r{Ok(Some(i))=>identity=Some(i),Ok(None)=>{if logged_out{identity=None;}},Err(e)=>{let error=match failed_nonce{Some(nonce)=>Event::Rejected{nonce,message:e.to_string()},None=>Event::Error{message:e.to_string()}};send_frame(&mut sink,Message::Text(serde_json::to_string(&error)?.into())).await?;},}
 },
 Message::Binary(b)=>{if let Some((u,g))=identity{let packets={let mut s=state.lock().unwrap();if s.peers.get(&u).is_some_and(|p|p.generation==g){s.relay(u,&b)}else{vec![]}};for(a,b)in packets{let _=udp.send_to(&b,a).await;}}},
 Message::Ping(b)=>send_frame(&mut sink,Message::Pong(b)).await?, Message::Pong(_)=>{},Message::Close(_)=>break,_=>{}
 }}
 }}Ok(())}.await;
    if let Some((u, g)) = identity {
        state.lock().unwrap().disconnect(u, g);
    }
    let _ = tokio::time::timeout(Duration::from_secs(5), sink.close()).await;
    result
}

async fn send_frame<S: futures_util::Sink<Message> + Unpin>(
    sink: &mut S,
    frame: Message,
) -> Result<()>
where
    S::Error: std::error::Error + Send + Sync + 'static,
{
    tokio::time::timeout(Duration::from_secs(5), sink.send(frame)).await??;
    Ok(())
}
