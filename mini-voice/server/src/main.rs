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
    let mut server = Server::open(&c.database, udp.local_addr()?.port())?;
    server.room_creator_username = c.room_creator_username.clone();
    server.room_creation_password_hash = c.room_creation_password_hash.to_ascii_lowercase();
    let state = Arc::new(Mutex::new(server));
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
    let avatar_slots = Arc::new(Semaphore::new(1));
    let attempts = Arc::new(Mutex::new(
        HashMap::<std::net::IpAddr, (Instant, u32)>::new(),
    ));
    loop {
        tokio::select! {result=shutdown_signal()=>{result?;break;},accept=listener.accept()=>{let(tcp,addr)=accept?;let Ok(permit)=connections.clone().try_acquire_owned() else{continue;};
        let allowed={let mut a=attempts.lock().unwrap();a.retain(|_,v|v.0.elapsed()<Duration::from_secs(60));let v=a.entry(addr.ip()).or_insert((Instant::now(),0));v.1+=1;v.1<=20};if !allowed{continue;}
        let acceptor=acceptor.clone();let s=state.clone();let udp=udp.clone();let slots=auth_slots.clone();let avatar_slots=avatar_slots.clone();tokio::spawn(async move{let _permit=permit;let result:Result<()>=async{
        let tls=tokio::time::timeout(Duration::from_secs(10),acceptor.accept(tcp)).await??;
        let config=WebSocketConfig::default().max_message_size(Some(MAX_FRAME)).max_frame_size(Some(MAX_FRAME));
        let ws=tokio::time::timeout(Duration::from_secs(10),accept_async_with_config(tls,Some(config))).await??;
        serve(ws,s,udp,slots,avatar_slots,addr).await}.await;if let Err(e)=result{eprintln!("Connection {addr}: {e}");}});
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
    avatar_slots: Arc<Semaphore>,
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
    let mut avatar_chunks = 0;
    let result:Result<()>=async{loop{tokio::select!{
 _=heartbeat.tick()=>{if last.elapsed()>Duration::from_secs(35){break;}
 if let Some((u,g))=identity{let mut s=state.lock().unwrap();s.expire_rejoins();if !s.peers.get(&u).is_some_and(|p|p.generation==g&&p.expiry>now()&&!p.overloaded.load(std::sync::atomic::Ordering::Relaxed))||tx.capacity()==0{break;}}send_frame(&mut sink,Message::Ping(vec![].into())).await?;},
 out=rx.recv()=>{match out{Some(Out::Json(e))=>send_frame(&mut sink,Message::Text(serde_json::to_string(&e)?.into())).await?,Some(Out::Audio(b))=>send_frame(&mut sink,Message::Binary(b.into())).await?,Some(Out::Replaced)=>{send_frame(&mut sink,Message::Text(serde_json::to_string(&Event::Error{message:"Signed in elsewhere".into()})?.into())).await?;break;},_=>break,}},
 incoming=stream.next()=>{let Some(incoming)=incoming else{break;};let msg=incoming?;last=Instant::now();match msg {
 Message::Text(text)=>{
 if tick.elapsed()>=Duration::from_secs(60){tick=Instant::now();requests=0;avatar_chunks=0;}
 let req=match serde_json::from_str::<Request>(&text){Ok(r)=>r,Err(_)=>{send_frame(&mut sink,Message::Text(serde_json::to_string(&Event::Error{message:"Malformed request".into()})?.into())).await?;continue;}};
 if matches!(req,Request::AvatarChunk{..}){avatar_chunks+=1;if avatar_chunks>1024{break;}}else{requests+=1;if requests>120{break;}}
 let s=state.clone();let tx2=tx.clone();let auth=identity.is_none();if auth{auth_attempts+=1;if auth_attempts>5{break;}}
 let failed_nonce=match &req{Request::Send{nonce,..}=>Some(nonce.clone()),_=>None};
 let logged_out=matches!(req,Request::Logout {});
 if let Some(who)=identity {if let Request::AvatarChunk{total,offset,data}=req {
   if let Err(error)=queue_avatar_upload(state.clone(),avatar_slots.clone(),who,(total,offset,data)){send_frame(&mut sink,Message::Text(serde_json::to_string(&Event::Error{message:error.to_string()})?.into())).await?;}
   continue;
 }}
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

// Keep image decoding off both the shared state lock and this connection's
// signaling loop, so TLS-fallback voice continues during avatar normalization.
fn queue_avatar_upload(
    state: Arc<Mutex<Server>>,
    slots: Arc<Semaphore>,
    (u, g): (i64, [u8; 16]),
    (total, offset, data): (usize, usize, String),
) -> Result<()> {
    let (bytes, epoch) = {
        let mut state = state.lock().unwrap();
        anyhow::ensure!(
            state
                .peers
                .get(&u)
                .is_some_and(|p| p.generation == g && p.expiry > now()),
            "Session replaced or expired"
        );
        let bytes = state.avatar_chunk_work(u, total, offset, data)?;
        (bytes, state.peers[&u].avatar_epoch)
    };
    if let Some(bytes) = bytes {
        // Never queue unbounded full images behind the decoder.
        let permit = slots
            .try_acquire_owned()
            .map_err(|_| anyhow::anyhow!("Avatar processing busy; upload again"))?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let result = (|| -> Result<()> {
                let optimized = mini_server::optimize_avatar(&bytes)?;
                let mut s = state.lock().unwrap();
                anyhow::ensure!(
                    s.peers
                        .get(&u)
                        .is_some_and(|p| p.generation == g && p.expiry > now()),
                    "Session replaced or expired"
                );
                if s.peers[&u].avatar_epoch != epoch {
                    return Ok(());
                }
                s.save_avatar(u, optimized)
            })();
            if let Err(error) = result {
                let s = state.lock().unwrap();
                if s.peers.get(&u).is_some_and(|p| p.generation == g) {
                    s.emit(
                        u,
                        Event::Error {
                            message: error.to_string(),
                        },
                    );
                }
            }
        });
    }
    Ok(())
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

#[cfg(test)]
mod avatar_races {
    use super::*;
    #[tokio::test]
    async fn removing_avatar_cancels_inflight_normalization() {
        let mut server = Server::open(":memory:", 4434).unwrap();
        let (tx, _rx) = mpsc::channel(128);
        let who = server
            .auth_verified(
                mini_server::VerifiedAuth::Register("profile_race".into(), "unused".into()),
                tx,
            )
            .unwrap();
        let state = Arc::new(Mutex::new(server));
        let slots = Arc::new(Semaphore::new(1));
        let mut image = std::io::Cursor::new(Vec::new());
        image::RgbImage::new(4096, 1024)
            .write_to(&mut image, image::ImageFormat::Png)
            .unwrap();
        let bytes = image.into_inner();
        for (i, chunk) in bytes.chunks(AVATAR_CHUNK).enumerate() {
            queue_avatar_upload(
                state.clone(),
                slots.clone(),
                who,
                (bytes.len(), i * AVATAR_CHUNK, hex::encode(chunk)),
            )
            .unwrap();
        }
        state
            .lock()
            .unwrap()
            .handle(who.0, Request::AvatarRemove {})
            .unwrap();
        let _finished = tokio::time::timeout(Duration::from_secs(10), slots.acquire())
            .await
            .unwrap()
            .unwrap();
        assert!(state.lock().unwrap().user(who.0).unwrap().avatar.is_none());
    }
}
