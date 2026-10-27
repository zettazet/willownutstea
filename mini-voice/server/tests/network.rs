//! Real TLS WebSocket and encrypted UDP integration; never disables certificate verification.
use futures_util::{SinkExt, StreamExt};
use mini_shared::*;
use std::{
    process::{Child, Command, Stdio},
    sync::Arc,
    time::Duration,
};
use tokio::{
    net::{TcpStream, UdpSocket},
    time::timeout,
};
use tokio_rustls::rustls;
use tokio_tungstenite::{tungstenite::Message, Connector, MaybeTlsStream, WebSocketStream};
fn server_command() -> Command {
    Command::new(
        std::env::var_os("MINIVOICE_TEST_SERVER")
            .unwrap_or_else(|| env!("CARGO_BIN_EXE_mini-server").into()),
    )
}
type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;
struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
async fn send(ws: &mut Ws, r: Request) {
    ws.send(Message::Text(serde_json::to_string(&r).unwrap().into()))
        .await
        .unwrap();
}
async fn event(ws: &mut Ws, mut predicate: impl FnMut(&Event) -> bool) -> Event {
    timeout(Duration::from_secs(10), async {
        loop {
            match ws.next().await.unwrap().unwrap() {
                Message::Text(t) => {
                    let e: Event = serde_json::from_str(&t).unwrap();
                    if predicate(&e) {
                        return e;
                    }
                }
                Message::Ping(b) => ws.send(Message::Pong(b)).await.unwrap(),
                _ => {}
            }
        }
    })
    .await
    .unwrap()
}
async fn connect(url: &str, tls: Arc<rustls::ClientConfig>) -> Ws {
    tokio_tungstenite::connect_async_tls_with_config(url, None, false, Some(Connector::Rustls(tls)))
        .await
        .unwrap()
        .0
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn eight_clients_tls_messaging_calls_udp_and_reconnect() {
    let dir = tempfile::tempdir().unwrap();
    let key = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    std::fs::write(dir.path().join("cert.pem"), key.cert.pem()).unwrap();
    std::fs::write(dir.path().join("key.pem"), key.signing_key.serialize_pem()).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let sock = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let udp_port = sock.local_addr().unwrap().port();
    drop(sock);
    let cfg = serde_json::json!({"listen":format!("127.0.0.1:{port}"),"udp_listen":format!("127.0.0.1:{udp_port}"),"database":"db.sqlite","tls_cert":"cert.pem","tls_key":"key.pem"});
    std::fs::write(dir.path().join("server.json"), cfg.to_string()).unwrap();
    let checked = server_command()
        .args(["--check-config", "server.json"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    assert!(!dir.path().join("db.sqlite").exists());
    let mut invalid = cfg.clone();
    invalid["listen"] = serde_json::json!("not-an-address");
    std::fs::write(dir.path().join("invalid.json"), invalid.to_string()).unwrap();
    let checked = server_command()
        .args(["--check-config", "invalid.json"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(!checked.status.success());
    assert!(String::from_utf8_lossy(&checked.stderr).contains("listen must be an IP:port"));
    let mut child = Process(
        server_command()
            .arg("server.json")
            .current_dir(dir.path())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let mut ready = false;
    for _ in 0..100 {
        if TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
            ready = true;
            break;
        }
        assert!(child.0.try_wait().unwrap().is_none(), "server exited");
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    assert!(ready);
    let mut roots = rustls::RootCertStore::empty();
    roots.add(key.cert.der().clone()).unwrap();
    let tls = Arc::new(
        rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth(),
    );
    let url = format!("wss://localhost:{port}");
    let mut sockets = vec![];
    let mut ids = vec![];
    let mut tokens = vec![];
    let mut voices = vec![];
    for i in 0..8 {
        let mut ws = connect(&url, tls.clone()).await;
        send(
            &mut ws,
            Request::Register {
                username: format!("friend{i}"),
                password: "correct horse battery".into(),
            },
        )
        .await;
        let auth = event(&mut ws, |e| matches!(e, Event::Auth { .. })).await;
        if let Event::Auth {
            user,
            token,
            voice_id,
            voice_key,
            ..
        } = auth
        {
            ids.push(user.id);
            tokens.push(token);
            voices.push((voice_id, voice_key));
        }
        sockets.push(ws);
    }
    for i in 1..8 {
        send(
            &mut sockets[0],
            Request::FriendRequest {
                username: format!("friend{i}"),
            },
        )
        .await;
        event(
            &mut sockets[i],
            |e| matches!(e,Event::State{state} if state.incoming.iter().any(|u|u.id==ids[0])),
        )
        .await;
        send(&mut sockets[i], Request::FriendAccept { user: ids[0] }).await;
        event(
            &mut sockets[0],
            |e| matches!(e,Event::State{state}if state.friends.iter().any(|u|u.id==ids[i])),
        )
        .await;
    }
    send(
        &mut sockets[0],
        Request::Conversation {
            users: ids[1..].to_vec(),
        },
    )
    .await;
    let state = event(
        &mut sockets[0],
        |e| matches!(e,Event::State{state}if !state.conversations.is_empty()),
    )
    .await;
    let c = if let Event::State { state } = state {
        state.conversations[0].id
    } else {
        unreachable!()
    };
    let message = Request::Send {
        conversation: c,
        nonce: "abcd".repeat(8),
        text: "over real TLS".into(),
    };
    send(&mut sockets[0], message.clone()).await;
    event(
        &mut sockets[1],
        |e| matches!(e,Event::Message{message}if message.text=="over real TLS"),
    )
    .await;
    event(&mut sockets[0], |e| matches!(e, Event::Ack { .. })).await;
    send(&mut sockets[0], message).await;
    event(&mut sockets[0], |e| matches!(e, Event::Ack { .. })).await;
    send(
        &mut sockets[0],
        Request::History {
            conversation: c,
            before: None,
        },
    )
    .await;
    let e = event(&mut sockets[0], |e| matches!(e, Event::History { .. })).await;
    if let Event::History { messages, .. } = e {
        assert_eq!(messages.len(), 1);
    }
    send(&mut sockets[0], Request::Call { conversation: c }).await;
    let e = event(
        &mut sockets[0],
        |e| matches!(e,Event::State{state}if !state.calls.is_empty()),
    )
    .await;
    let call = if let Event::State { state } = e {
        state.calls[0].id
    } else {
        unreachable!()
    };
    send(&mut sockets[0], Request::Toggle {}).await;
    for socket in sockets.iter_mut().skip(1) {
        send(socket, Request::Join { call }).await;
    }
    let state = event(
        &mut sockets[0],
        |e| matches!(e,Event::State{state}if state.calls[0].participants.len()==8 && !state.calls[0].participants[0].labeled && state.calls[0].participants[1..].iter().all(|p|p.labeled)),
    )
    .await;
    if let Event::State { state } = state {
        assert!(!state.calls[0].participants[0].labeled);
        assert!(state.calls[0].participants[1..].iter().all(|p| p.labeled));
    }
    // Simultaneous activations combine per-user contributions, without one user's toggle erasing another's.
    send(&mut sockets[1], Request::Toggle {}).await;
    event(
        &mut sockets[0],
        |e| matches!(e,Event::State{state}if state.calls[0].participants.iter().all(|p|p.labeled)),
    )
    .await;
    send(&mut sockets[0], Request::Toggle {}).await;
    event(&mut sockets[0],|e|matches!(e,Event::State{state}if state.calls[0].participants[0].labeled&&!state.calls[0].participants[1].labeled)).await;
    // Actual encrypted UDP relay and authenticated NAT address registration.
    let a = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let b = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    a.connect(("127.0.0.1", udp_port)).await.unwrap();
    b.connect(("127.0.0.1", udp_port)).await.unwrap();
    let mut au = VoiceCipher::new(&voices[0].1, voices[0].0, b"up");
    let mut bu = VoiceCipher::new(&voices[1].1, voices[1].0, b"up");
    let mut bd = VoiceCipher::new(&voices[1].1, voices[1].0, b"down");
    let mut buf = [0u8; 1400];
    b.send(&bu.seal(&[]).unwrap()).await.unwrap();
    let n = timeout(Duration::from_secs(2), b.recv(&mut buf))
        .await
        .unwrap()
        .unwrap();
    assert!(bd.open(&buf[..n]).unwrap().is_empty());
    send(&mut sockets[1], Request::VoiceTransport { udp: true }).await;
    event(&mut sockets[1], |e| matches!(e, Event::Pong)).await;
    let mut voice = call.to_be_bytes().to_vec();
    voice.extend_from_slice(&0u64.to_be_bytes());
    voice.extend_from_slice(&[0xf8, 0xff, 0xfe]);
    let p = au.seal(&voice).unwrap();
    a.send(&p).await.unwrap();
    let n = timeout(Duration::from_secs(2), b.recv(&mut buf))
        .await
        .unwrap()
        .unwrap();
    let data = bd.open(&buf[..n]).unwrap();
    assert_eq!(&data[8..16], &ids[0].to_be_bytes());
    assert_eq!(&data[24..], &[0xf8, 0xff, 0xfe]);
    a.send(&p).await.unwrap();
    assert!(timeout(Duration::from_millis(150), b.recv(&mut buf))
        .await
        .is_err());
    send(
        &mut sockets[0],
        Request::Controls {
            mic_muted: true,
            deafened: false,
        },
    )
    .await;
    event(
        &mut sockets[0],
        |e| matches!(e,Event::State{state}if state.calls[0].participants[0].mic_muted),
    )
    .await;
    a.send(&au.seal(&voice).unwrap()).await.unwrap();
    assert!(timeout(Duration::from_millis(150), b.recv(&mut buf))
        .await
        .is_err());
    send(
        &mut sockets[0],
        Request::Controls {
            mic_muted: false,
            deafened: false,
        },
    )
    .await;
    send(
        &mut sockets[1],
        Request::Controls {
            mic_muted: false,
            deafened: true,
        },
    )
    .await;
    event(&mut sockets[1],|e|matches!(e,Event::State{state}if state.calls[0].participants.get(1).is_some_and(|p|p.deafened))).await;
    a.send(&au.seal(&voice).unwrap()).await.unwrap();
    assert!(timeout(Duration::from_millis(150), b.recv(&mut buf))
        .await
        .is_err());

    // Seven simultaneous senders, including a deafened sender, reach the eighth user.
    for socket in sockets.iter_mut().take(7) {
        socket
            .send(Message::Binary(voice.clone().into()))
            .await
            .unwrap();
    }
    let mut heard = std::collections::HashSet::new();
    timeout(Duration::from_secs(3), async {
        while heard.len() < 7 {
            match sockets[7].next().await.unwrap().unwrap() {
                Message::Binary(b) => {
                    heard.insert(i64::from_be_bytes(b[8..16].try_into().unwrap()));
                }
                Message::Ping(b) => sockets[7].send(Message::Pong(b)).await.unwrap(),
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    assert!(ids[..7].iter().all(|id| heard.contains(id)));
    sockets[0]
        .send(Message::Text("{malformed".into()))
        .await
        .unwrap();
    event(
        &mut sockets[0],
        |e| matches!(e,Event::Error{message}if message=="Malformed request"),
    )
    .await;
    sockets[1].close(None).await.unwrap();
    let mut resumed = connect(&url, tls.clone()).await;
    send(
        &mut resumed,
        Request::Resume {
            token: tokens[1].clone(),
        },
    )
    .await;
    event(&mut resumed, |e| matches!(e, Event::Auth { .. })).await;
    let e = event(&mut resumed, |e| matches!(e, Event::State { .. })).await;
    if let Event::State { state } = e {
        assert!(state
            .calls
            .iter()
            .all(|c| c.participants.iter().all(|p| p.user.id != ids[1])));
        assert_eq!(state.conversations.len(), 1);
    }
    send(&mut resumed, Request::Logout {}).await;
    event(&mut resumed, |e| matches!(e, Event::LoggedOut)).await;
    send(
        &mut resumed,
        Request::Resume {
            token: tokens[1].clone(),
        },
    )
    .await;
    event(&mut resumed, |e| matches!(e, Event::Error { .. })).await;
    // The actual client networking engine reconnects through a TCP fault-injection proxy.
    // TLS remains end to end; the proxy does not terminate or inspect encryption.
    let proxy = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_port = proxy.local_addr().unwrap().port();
    let (cut, _) = tokio::sync::broadcast::channel::<()>(8);
    let cuts = cut.clone();
    let proxy_task = tokio::spawn(async move {
        loop {
            let (mut inbound, _) = proxy.accept().await.unwrap();
            let mut outbound = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
            let mut cuts = cuts.subscribe();
            tokio::spawn(async move {
                tokio::select! {_=tokio::io::copy_bidirectional(&mut inbound,&mut outbound)=>{},_=cuts.recv()=>{}}
            });
        }
    });
    sockets[2].close(None).await.unwrap();
    event(&mut sockets[0],|e|matches!(e,Event::State{state}if state.calls[0].participants.iter().all(|p|p.user.id!=ids[2]))).await;
    let (ui, events) = std::sync::mpsc::sync_channel(128);
    let mixer = Arc::new(std::sync::Mutex::new(mini_client::audio::Mixer::new()));
    let bridge = mini_client::network::start(
        &tokio::runtime::Handle::current(),
        format!("wss://localhost:{proxy_port}"),
        Some(tokens[2].clone()),
        ui,
        Arc::new(|| {}),
        mixer,
        Some(Connector::Rustls(tls.clone())),
    );
    client_event(&events, |e| {
        matches!(e, mini_client::network::UiEvent::Server(Event::Auth { .. }))
    })
    .await;
    bridge.requests.send(Request::Join { call }).await.unwrap();
    client_event(&events,|e|matches!(e,mini_client::network::UiEvent::Server(Event::State{state})if state.calls.iter().any(|c|c.participants.iter().any(|p|p.user.id==ids[2])))).await;
    bridge
        .requests
        .send(Request::Controls {
            mic_muted: true,
            deafened: true,
        })
        .await
        .unwrap();
    client_event(&events,|e|matches!(e,mini_client::network::UiEvent::Server(Event::State{state})if state.calls.iter().any(|c|c.participants.iter().any(|p|p.user.id==ids[2]&&p.mic_muted&&p.deafened)))).await;
    bridge
        .requests
        .send(Request::Send {
            conversation: c,
            nonce: "eeff".repeat(8),
            text: "retry during interruption".into(),
        })
        .await
        .unwrap();
    cut.send(()).unwrap();
    client_event(
        &events,
        |e| matches!(e,mini_client::network::UiEvent::Status(s)if s.starts_with("Disconnected")),
    )
    .await;
    client_event(&events, |e| {
        matches!(e, mini_client::network::UiEvent::Server(Event::Auth { .. }))
    })
    .await;
    client_event(&events,|e|matches!(e,mini_client::network::UiEvent::Server(Event::State{state})if state.calls.iter().any(|c|c.participants.iter().any(|p|p.user.id==ids[2]&&p.mic_muted&&p.deafened)))).await;
    bridge
        .requests
        .send(Request::History {
            conversation: c,
            before: None,
        })
        .await
        .unwrap();
    client_event(&events,|e|matches!(e,mini_client::network::UiEvent::Server(Event::History{messages,..})if messages.iter().filter(|m|m.text=="retry during interruption").count()==1)).await;
    // A page of maximum-size, JSON-escaped messages exceeds the old 128 KiB
    // client receive limit. Exercise the actual client parser, not only the DB.
    let large_text = "\"".repeat(MAX_TEXT);
    for i in 0..PAGE {
        let nonce = format!("{i:032x}");
        send(
            &mut sockets[0],
            Request::Send {
                conversation: c,
                nonce: nonce.clone(),
                text: large_text.clone(),
            },
        )
        .await;
        event(
            &mut sockets[0],
            |e| matches!(e,Event::Ack{nonce:n,..}if *n==nonce),
        )
        .await;
    }
    bridge
        .requests
        .send(Request::History {
            conversation: c,
            before: None,
        })
        .await
        .unwrap();
    let mut oldest = None;
    client_event(&events, |e| {
        if let mini_client::network::UiEvent::Server(Event::History { messages, .. }) = e {
            assert_eq!(messages.len(), PAGE);
            assert!(messages.iter().all(|m| m.text == large_text));
            oldest = messages.first().map(|m| m.id);
            true
        } else {
            false
        }
    })
    .await;
    bridge
        .requests
        .send(Request::History {
            conversation: c,
            before: oldest,
        })
        .await
        .unwrap();
    client_event(&events,|e|matches!(e,mini_client::network::UiEvent::Server(Event::History{messages,..})if messages.len()==2 && messages[0].text=="over real TLS" && messages[1].text=="retry during interruption")).await;
    bridge.task.abort();
    proxy_task.abort();
    // A recipient which has no UDP mapping gets real voice bytes over the TLS socket.
    sockets[0]
        .send(Message::Binary(voice.clone().into()))
        .await
        .unwrap();
    timeout(Duration::from_secs(3), async {
        loop {
            match sockets[3].next().await.unwrap().unwrap() {
                Message::Binary(data) => {
                    assert_eq!(&data[24..], &[0xf8, 0xff, 0xfe]);
                    break;
                }
                Message::Ping(b) => sockets[3].send(Message::Pong(b)).await.unwrap(),
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    #[cfg(unix)]
    {
        assert!(Command::new("kill")
            .args(["-TERM", &child.0.id().to_string()])
            .status()
            .unwrap()
            .success());
        let status = timeout(Duration::from_secs(10), async {
            loop {
                if let Some(status) = child.0.try_wait().unwrap() {
                    break status;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        assert!(status.success(), "SIGTERM must exit cleanly");
        let db = rusqlite::Connection::open(dir.path().join("db.sqlite")).unwrap();
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM messages", [], |r| r
                .get::<_, usize>(0))
                .unwrap(),
            PAGE + 2
        );
        assert_eq!(
            db.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
    }
}

async fn client_event(
    rx: &std::sync::mpsc::Receiver<mini_client::network::UiEvent>,
    mut predicate: impl FnMut(&mini_client::network::UiEvent) -> bool,
) {
    timeout(Duration::from_secs(12), async {
        loop {
            while let Ok(e) = rx.try_recv() {
                if predicate(&e) {
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
