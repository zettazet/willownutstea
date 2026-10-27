use crate::audio::{AudioPacket, Mixer};
use futures_util::{SinkExt, StreamExt};
use mini_shared::*;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{net::UdpSocket, sync::mpsc};
use tokio_tungstenite::{
    tungstenite::{protocol::WebSocketConfig, Message},
    Connector,
};

#[derive(Debug)]
pub enum UiEvent {
    Server(Event),
    Status(String),
}
pub struct Bridge {
    /// Local connection origin; never supplied by the remote server.
    pub endpoint: String,
    pub requests: mpsc::Sender<Request>,
    pub audio: mpsc::Sender<AudioPacket>,
    pub avatars: mpsc::Sender<Vec<u8>>,
    pub task: tokio::task::JoinHandle<()>,
}
fn notify(
    tx: &std::sync::mpsc::SyncSender<UiEvent>,
    wake: &Arc<dyn Fn() + Send + Sync>,
    event: UiEvent,
) -> bool {
    let ok = tx.try_send(event).is_ok();
    wake();
    ok
}
/// An optional Connector supplies an explicit trusted CA for integration tests.
/// Production uses the Windows certificate store. Neither path bypasses TLS verification.
#[allow(clippy::too_many_arguments)]
pub fn start(
    rt: &tokio::runtime::Handle,
    endpoint: String,
    initial_token: Option<String>,
    ui: std::sync::mpsc::SyncSender<UiEvent>,
    wake: Arc<dyn Fn() + Send + Sync>,
    mixer: Arc<Mutex<Mixer>>,
    connector: Option<Connector>,
) -> Bridge {
    // Select explicitly: workspace feature unification may enable another backend.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let (tx, rx) = mpsc::channel(32);
    let (atx, arx) = mpsc::channel(3);
    let (avatar_tx, avatar_rx) = mpsc::channel(1);
    let connected_endpoint = endpoint.clone();
    let task = rt.spawn(run(
        endpoint,
        initial_token,
        ui,
        wake,
        mixer,
        connector,
        rx,
        arx,
        avatar_rx,
    ));
    Bridge {
        endpoint: connected_endpoint,
        requests: tx,
        audio: atx,
        avatars: avatar_tx,
        task,
    }
}
#[allow(clippy::too_many_arguments)]
async fn run(
    endpoint: String,
    mut token: Option<String>,
    ui: std::sync::mpsc::SyncSender<UiEvent>,
    wake: Arc<dyn Fn() + Send + Sync>,
    mixer: Arc<Mutex<Mixer>>,
    connector: Option<Connector>,
    mut rx: mpsc::Receiver<Request>,
    mut arx: mpsc::Receiver<AudioPacket>,
    mut avatar_rx: mpsc::Receiver<Vec<u8>>,
) {
    let _url = match url::Url::parse(&endpoint) {
        Ok(u)
            if u.scheme() == "wss"
                && u.host_str().is_some()
                && u.username().is_empty()
                && u.password().is_none() =>
        {
            u
        }
        _ => {
            notify(
                &ui,
                &wake,
                UiEvent::Status("Use wss://hostname:4433".into()),
            );
            return;
        }
    };
    let mut pending: VecDeque<Request> = VecDeque::new();
    let mut backoff = 1;
    let mut desired_call = None;
    let mut mic_muted = false;
    let mut deafened = false;
    loop {
        notify(&ui, &wake, UiEvent::Status("Connecting…".into()));
        let mut failure = "Connection closed".to_string();
        let config = WebSocketConfig::default()
            .max_message_size(Some(MAX_EVENT))
            .max_frame_size(Some(MAX_EVENT));
        let connection = tokio::time::timeout(
            Duration::from_secs(10),
            tokio_tungstenite::connect_async_tls_with_config(
                &endpoint,
                Some(config),
                false,
                connector.clone(),
            ),
        )
        .await;
        if let Ok(Ok((socket, _))) = connection {
            let peer_ip = match socket.get_ref() {
                tokio_tungstenite::MaybeTlsStream::Rustls(s) => {
                    s.get_ref().0.peer_addr().ok().map(|a| a.ip())
                }
                _ => None,
            };
            let (mut sink, mut stream) = socket.split();
            notify(&ui, &wake, UiEvent::Status("Connected; sign in".into()));
            if let Some(t) = &token {
                let _ = send_request(&mut sink, &Request::Resume { token: t.clone() }).await;
            }
            let mut active_upload: Option<(Vec<u8>, usize)> = None;
            let mut avatar_send_after = tokio::time::Instant::now();
            let mut authenticated = false;
            let mut me = 0;
            let mut room: Option<u64> = None;
            let mut rejoining = false;
            let mut sequence = 0u64;
            let mut heartbeat = tokio::time::interval(Duration::from_secs(1));
            let mut last = Instant::now();
            let mut last_udp = Instant::now() - Duration::from_secs(30);
            let mut udp: Option<UdpSocket> = None;
            let mut up: Option<VoiceCipher> = None;
            let mut down: Option<VoiceCipher> = None;
            let mut buffer = [0u8; 1400];
            let mut awaiting_resume = token.is_some();
            let mut receiving_udp = false;
            let mut heartbeat_ticks = 0u32;
            loop {
                tokio::select! {
                    upload=avatar_rx.recv()=>{
                        if let Some(bytes)=upload {
                            if authenticated && !bytes.is_empty() && bytes.len()<=MAX_AVATAR_UPLOAD && active_upload.is_none() {
                                active_upload=Some((bytes,0));
                            } else {notify(&ui,&wake,UiEvent::Status("Avatar upload requires sign-in; wait for current upload".into()));}
                        }
                    },
                    _=tokio::time::sleep_until(avatar_send_after),if active_upload.is_some()=>{
                        let (bytes,offset)=active_upload.as_mut().unwrap();
                        let chunk=if room.is_some(){1024}else{AVATAR_CHUNK};
                        let end=(*offset+chunk).min(bytes.len());
                        let request=Request::AvatarChunk{total:bytes.len(),offset:*offset,data:hex::encode(&bytes[*offset..end])};
                        if send_request(&mut sink,&request).await.is_err(){break;}
                        *offset=end;
                        avatar_send_after=tokio::time::Instant::now()+if room.is_some(){Duration::from_millis(75)}else{Duration::ZERO};
                        if end==bytes.len(){active_upload=None;}
                    },
                    _=heartbeat.tick()=>{
                        if last.elapsed()>Duration::from_secs(35) {break;}
                        let use_udp=room.is_some() && last_udp.elapsed()<Duration::from_secs(3);
                        if authenticated && use_udp!=receiving_udp{receiving_udp=use_udp;if send_request(&mut sink,&Request::VoiceTransport{udp:use_udp}).await.is_err(){break;}}
                        if room.is_some(){if let(Some(s),Some(cipher))=(&udp,&mut up) {
                            if let Some(p)=cipher.seal(&[]) {let _=s.send(&p).await;}
                        }}
                        heartbeat_ticks+=1;
                        if heartbeat_ticks.is_multiple_of(10) && send_frame(&mut sink,Message::Ping(vec![].into())).await.is_err(){break;}
                    },
                    cmd=rx.recv()=>{
                        let Some(cmd)=cmd else{return;};
                        if matches!(cmd,Request::Login{..}|Request::Register{..}){active_upload=None;pending.clear();desired_call=None;}
                        if let Request::Send{nonce,..}=&cmd {
                            if pending.len()>=32 || (!authenticated && token.is_none()) {
                                notify(&ui,&wake,UiEvent::Server(Event::Rejected{nonce:nonce.clone(),message:"Outbox full or not signed in".into()}));continue;
                            }
                            pending.push_back(cmd.clone());
                            if !authenticated {continue;}
                        }
                        if !authenticated && !matches!(cmd,Request::Login{..}|Request::Register{..}) {
                            if matches!(cmd,Request::Logout {}){token=None;pending.clear();desired_call=None;notify(&ui,&wake,UiEvent::Server(Event::LoggedOut));break;}
                            if matches!(cmd,Request::Leave {}){desired_call=None;}
                            notify(&ui,&wake,UiEvent::Status("Sign in first".into()));continue;
                        }
                        if let Request::Controls{mic_muted:m,deafened:d}=&cmd {
                            mic_muted = *m;deafened = *d;
                            if *m {while arx.try_recv().is_ok(){}}
                        }
                        if matches!(cmd,Request::Leave {}){desired_call=None;room=None;while arx.try_recv().is_ok(){}}
                        if matches!(cmd,Request::Logout {}){active_upload=None;token=None;pending.clear();desired_call=None;}
                        if send_request(&mut sink,&cmd).await.is_err(){break;}
                    },
                    audio=arx.recv(),if authenticated=>{
                        if let Some(p)=audio {
                            let Some(call)=room else{continue;};
                            if mic_muted {continue;}
                            let mut payload=call.to_be_bytes().to_vec();
                            payload.extend_from_slice(&sequence.to_be_bytes());
                            sequence=sequence.wrapping_add(1);
                            payload.extend_from_slice(&p.bytes[..p.len]);
                            if last_udp.elapsed()<Duration::from_secs(3) {
                                if let(Some(s),Some(cipher))=(&udp,&mut up) {
                                    if let Some(b)=cipher.seal(&payload){let _=s.send(&b).await;}
                                }
                            } else if send_frame(&mut sink,Message::Binary(payload.into())).await.is_err(){break;}
                        }
                    },
                    received=async{if let Some(s)=&udp{s.recv(&mut buffer).await}else{std::future::pending().await}},if authenticated=>{
                        if let Ok(n)=received {
                            if let Some(cipher)=&mut down {
                                if let Some(b)=cipher.open(&buffer[..n]) {
                                    last_udp=Instant::now();
                                    if !deafened {receive_audio(&mixer,&b,room);}
                                }
                            }
                        }
                    },
                    msg=stream.next()=>{
                        let msg=match msg{Some(Ok(m))=>m,Some(Err(e))=>{failure=e.to_string();break;},None=>break};
                        last=Instant::now();
                        match msg {
                            Message::Text(t)=>{
                                let Ok(mut event)=serde_json::from_str::<Event>(&t) else{break;};
                                match &mut event {
                                    Event::Auth{user,token:t,voice_id,voice_key,udp_port,..}=>{
                                        me=user.id;token=Some(t.clone());authenticated=true;awaiting_resume=false;backoff=1;
                                        up=Some(VoiceCipher::new(voice_key,*voice_id,b"up"));
                                        down=Some(VoiceCipher::new(voice_key,*voice_id,b"down"));
                                        if let Some(ip)=peer_ip {
                                            let addr=std::net::SocketAddr::new(ip,*udp_port);
                                            let bind=if addr.is_ipv4(){"0.0.0.0:0"}else{"[::]:0"};
                                            if let Ok(s)=UdpSocket::bind(bind).await {if s.connect(addr).await.is_ok(){udp=Some(s);}}
                                        }
                                        for p in &pending {if send_request(&mut sink,p).await.is_err(){break;}}
                                        notify(&ui,&wake,UiEvent::Status("Online".into()));
                                    },
                                    Event::State{state}=>{
                                        let next=state.calls.iter_mut().find(|c|c.participants.iter().any(|p|p.user.id==me));
                                        if let Some(c)=next {
                                            if room!=Some(c.id) {
                                                if desired_call!=Some(c.id) {mic_muted=false;deafened=false;}
                                                room=Some(c.id);desired_call=room;rejoining=false;sequence=0;
                                                while arx.try_recv().is_ok(){}
                                                mixer.lock().unwrap().speakers.clear();
                                                if let Some(p)=c.participants.iter_mut().find(|p|p.user.id==me){p.mic_muted=mic_muted;p.deafened=deafened;}
                                                if send_request(&mut sink,&Request::Controls{mic_muted,deafened}).await.is_err(){break;}
                                            }
                                            mixer.lock().unwrap().speakers.retain(|id,_|c.participants.iter().any(|p|p.user.id==*id));
                                        } else {
                                            room=None;
                                            if let Some(call)=desired_call {
                                                if state.calls.iter().any(|c|c.id==call&&c.invited.contains(&me)) || state.voice_rooms.iter().any(|r|r.id==call) {
                                                    if !rejoining {rejoining=true;if send_request(&mut sink,&Request::Join{call}).await.is_err(){break;}}
                                                } else {desired_call=None;}
                                            }
                                        }
                                    },
                                    Event::Ack{nonce,..}|Event::Rejected{nonce,..}=>pending.retain(|r|!matches!(r,Request::Send{nonce:n,..}if n==nonce)),
                                    Event::Error{..} if awaiting_resume=>{
                                        token=None;awaiting_resume=false;desired_call=None;pending.clear();
                                        notify(&ui,&wake,UiEvent::Server(Event::LoggedOut));
                                    },
                                    Event::Error{message} if message=="Signed in elsewhere"=>{
                                        active_upload=None;token=None;authenticated=false;desired_call=None;pending.clear();
                                        notify(&ui,&wake,UiEvent::Server(Event::LoggedOut));
                                    },
                                    Event::Error{..} if active_upload.is_some()=>{active_upload=None;},
                                    Event::LoggedOut=>{active_upload=None;token=None;authenticated=false;pending.clear();desired_call=None;},
                                    _=>{}
                                }
                                if !notify(&ui,&wake,UiEvent::Server(event)){break;}
                            },
                            Message::Binary(b)=>{if !deafened{receive_audio(&mixer,&b,room);}},
                            Message::Ping(b)=>{if send_frame(&mut sink,Message::Pong(b)).await.is_err(){break;}},
                            Message::Close(_)=>break,
                            _=>{}
                        }
                    }
                }
            }
        } else {
            failure = match connection {
                Ok(Err(e)) => e.to_string(),
                Err(_) => "Connection timed out".into(),
                _ => unreachable!(),
            };
        }

        mixer.lock().unwrap().speakers.clear();
        notify(
            &ui,
            &wake,
            UiEvent::Status(format!(
                "Disconnected: {failure}; retry in {backoff}s. Audio paused."
            )),
        );
        let delay = tokio::time::sleep(Duration::from_secs(backoff));
        tokio::pin!(delay);
        loop {
            tokio::select! {
                _=&mut delay=>break,
                r=rx.recv()=>{
                    let Some(r)=r else{return;};
                    match r {
                        Request::Logout {}=>{token=None;pending.clear();desired_call=None;notify(&ui,&wake,UiEvent::Server(Event::LoggedOut));},
                        Request::Leave {}=>desired_call=None,
                        Request::Send{..} if pending.len()<32 && token.is_some()=>pending.push_back(r),
                    Request::Send{nonce,..}=>{notify(&ui,&wake,UiEvent::Server(Event::Rejected{nonce,message:"Outbox full or not signed in".into()}));},
                        _=>{notify(&ui,&wake,UiEvent::Status("Offline; retry this action after reconnect".into()));}
                    }
                }
            }
        }
        backoff = (backoff * 2).min(15);
    }
}
fn receive_audio(m: &Arc<Mutex<Mixer>>, b: &[u8], room: Option<u64>) {
    if b.len() > 24
        && b.len() <= MAX_OPUS + 24
        && room == Some(u64::from_be_bytes(b[..8].try_into().unwrap()))
    {
        let user = i64::from_be_bytes(b[8..16].try_into().unwrap());
        let seq = u64::from_be_bytes(b[16..24].try_into().unwrap());
        if let Ok(mut m) = m.lock() {
            let _ = m.packet(user, seq, &b[24..]);
        }
    }
}
async fn send_request<S: futures_util::Sink<Message> + Unpin>(
    sink: &mut S,
    r: &Request,
) -> Result<(), ()> {
    send_frame(
        sink,
        Message::Text(serde_json::to_string(r).map_err(|_| ())?.into()),
    )
    .await
}
async fn send_frame<S: futures_util::Sink<Message> + Unpin>(
    sink: &mut S,
    frame: Message,
) -> Result<(), ()> {
    tokio::time::timeout(Duration::from_secs(5), sink.send(frame))
        .await
        .map_err(|_| ())?
        .map_err(|_| ())
}
