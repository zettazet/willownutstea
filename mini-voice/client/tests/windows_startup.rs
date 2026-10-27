#![cfg(windows)]
//! Exercise the actual GUI executable with a saved endpoint, not a library-only build.
use std::{
    io::Read,
    net::TcpListener,
    process::{Child, Command},
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{Foundation::*, UI::WindowsAndMessaging::*};
struct Client(Child);
impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct Search {
    pid: u32,
    window: HWND,
}
unsafe extern "system" fn find(window: HWND, param: LPARAM) -> i32 {
    let search = &mut *(param as *mut Search);
    let mut pid = 0;
    GetWindowThreadProcessId(window, &mut pid);
    if pid == search.pid && !GetDlgItem(window, 141).is_null() {
        search.window = window;
        return 0;
    }
    1
}
fn wait_until(mut check: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !check() {
        assert!(Instant::now() < deadline, "GUI operation timed out");
        thread::sleep(Duration::from_millis(50));
    }
}
#[test]
fn saved_endpoint_startup_background_restore_and_exit() {
    let temp = std::env::temp_dir().join(format!(
        "mini-voice-gui-test-{}-{:016x}",
        std::process::id(),
        rand::random::<u64>()
    ));
    let settings = temp.join("MiniVoice");
    std::fs::create_dir_all(&settings).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    std::fs::write(
        settings.join("settings.json"),
        format!(r#"{{"server":"wss://127.0.0.1:{port}","token":[]}}"#),
    )
    .unwrap();
    let mut child = Client(
        Command::new(env!("CARGO_BIN_EXE_mini-client"))
            .env("LOCALAPPDATA", &temp)
            .spawn()
            .unwrap(),
    );
    let mut search = Search {
        pid: child.0.id(),
        window: std::ptr::null_mut(),
    };
    wait_until(|| {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "GUI exited during startup"
        );
        unsafe {
            EnumWindows(Some(find), &mut search as *mut _ as isize);
        }
        !search.window.is_null() && unsafe { IsWindowVisible(search.window) } != 0
    });
    let mut socket = None;
    wait_until(|| {
        socket = listener.accept().ok().map(|s| s.0);
        socket.is_some()
    });
    let mut socket = socket.unwrap();
    socket.set_nonblocking(false).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut header = [0; 5];
    socket
        .read_exact(&mut header)
        .expect("actual standalone EXE did not send TLS ClientHello");
    assert_eq!(header[0], 22);
    // A second registration must fail while the app owns global Insert.
    unsafe {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
        assert_eq!(
            RegisterHotKey(std::ptr::null_mut(), 913, MOD_NOREPEAT, VK_INSERT as u32),
            0
        );
    }
    // The existing settings file has no preferences: migration must use defaults.
    unsafe {
        PostMessageW(search.window, WM_COMMAND, 150, 0);
    }
    wait_until(|| unsafe { IsWindowVisible(GetDlgItem(search.window, 153)) } != 0);
    unsafe {
        assert_eq!(
            SendMessageW(GetDlgItem(search.window, 160), BM_GETCHECK, 0, 0),
            1
        );
        SendMessageW(GetDlgItem(search.window, 160), BM_SETCHECK, 0, 0);
        PostMessageW(search.window, WM_COMMAND, 160, 0);
        assert_eq!(
            SendMessageW(GetDlgItem(search.window, 161), BM_GETCHECK, 0, 0),
            1
        );
        assert_eq!(
            SendMessageW(GetDlgItem(search.window, 162), BM_GETCHECK, 0, 0),
            1
        );
        assert_eq!(
            SendMessageW(GetDlgItem(search.window, 163), WM_USER, 0, 0),
            10
        );
        for id in [161, 162] {
            SendMessageW(GetDlgItem(search.window, id), BM_SETCHECK, 0, 0);
            PostMessageW(search.window, WM_COMMAND, id as usize, 0);
        }
        SendMessageW(GetDlgItem(search.window, 163), WM_USER + 5, 1, 22);
        PostMessageW(
            search.window,
            WM_HSCROLL,
            8,
            GetDlgItem(search.window, 163) as isize,
        );
        let microphone = GetDlgItem(search.window, 153);
        let output = GetDlgItem(search.window, 154);
        assert_eq!(SendMessageW(microphone, WM_USER, 0, 0), 100);
        assert_eq!(SendMessageW(output, WM_USER, 0, 0), 100);
        assert!(
            SendMessageW(GetDlgItem(search.window, 151), CB_GETCOUNT, 0, 0) >= 1,
            "device combo {:?}, count {}",
            GetDlgItem(search.window, 151),
            SendMessageW(GetDlgItem(search.window, 151), CB_GETCOUNT, 0, 0)
        );
        SendMessageW(microphone, WM_USER + 5, 1, 150); // TBM_SETPOS
        PostMessageW(search.window, WM_HSCROLL, 8, microphone as isize); // TB_ENDTRACK
        SendMessageW(output, WM_USER + 5, 1, 40);
        PostMessageW(search.window, WM_HSCROLL, 8, output as isize);
        SendMessageW(GetDlgItem(search.window, 155), BM_SETCHECK, 0, 0);
        PostMessageW(search.window, WM_COMMAND, 155, 0);
    }
    wait_until(|| {
        std::fs::read(settings.join("settings.json"))
            .ok()
            .and_then(|data| serde_json::from_slice::<serde_json::Value>(&data).ok())
            .is_some_and(|value| {
                value["preferences"]["microphone"] == 150
                    && value["preferences"]["master"] == 40
                    && value["preferences"]["show_avatars"] == false
                    && value["preferences"]["voice_cleanup"] == false
                    && value["preferences"]["neural_cleanup"] == false
                    && value["preferences"]["notification_sounds"] == false
                    && value["preferences"]["noise_threshold_db"] == -38
            })
    });
    unsafe {
        PostMessageW(search.window, WM_COMMAND, 150, 0);
    }
    wait_until(|| unsafe { IsWindowVisible(GetDlgItem(search.window, 153)) } == 0);
    unsafe {
        PostMessageW(search.window, WM_CLOSE, 0, 0);
    }
    wait_until(|| unsafe { IsWindowVisible(search.window) } == 0);
    assert!(
        child.0.try_wait().unwrap().is_none(),
        "close must keep process alive"
    );
    // Exercise the same callback emitted by the notification-area icon.
    unsafe {
        PostMessageW(search.window, WM_APP + 4, 1, WM_LBUTTONUP as isize);
    }
    wait_until(|| unsafe { IsWindowVisible(search.window) } != 0);
    // Minimize must also preserve the connection and hide to tray.
    unsafe {
        PostMessageW(search.window, WM_SYSCOMMAND, SC_MINIMIZE as usize, 0);
    }
    wait_until(|| unsafe { IsWindowVisible(search.window) } == 0);
    unsafe {
        PostMessageW(search.window, WM_APP + 4, 1, WM_LBUTTONUP as isize);
    }
    wait_until(|| unsafe { IsWindowVisible(search.window) } != 0);
    // Exit app is a normal button, also usable when the tray is unavailable.
    unsafe {
        PostMessageW(search.window, WM_COMMAND, 141, 0);
    }
    wait_until(|| child.0.try_wait().unwrap().is_some());
    assert!(child.0.wait().unwrap().success());
    let mut restarted = Client(
        Command::new(env!("CARGO_BIN_EXE_mini-client"))
            .env("LOCALAPPDATA", &temp)
            .spawn()
            .unwrap(),
    );
    search.pid = restarted.0.id();
    search.window = std::ptr::null_mut();
    wait_until(|| {
        unsafe {
            EnumWindows(Some(find), &mut search as *mut _ as isize);
        }
        !search.window.is_null() && unsafe { IsWindowVisible(search.window) } != 0
    });
    unsafe {
        PostMessageW(search.window, WM_COMMAND, 150, 0);
    }
    wait_until(|| unsafe { IsWindowVisible(GetDlgItem(search.window, 153)) } != 0);
    unsafe {
        assert_eq!(
            SendMessageW(GetDlgItem(search.window, 153), WM_USER, 0, 0),
            150
        );
        assert_eq!(
            SendMessageW(GetDlgItem(search.window, 154), WM_USER, 0, 0),
            40
        );
        assert_eq!(
            SendMessageW(GetDlgItem(search.window, 155), BM_GETCHECK, 0, 0),
            0
        );
        assert_eq!(
            SendMessageW(GetDlgItem(search.window, 160), BM_GETCHECK, 0, 0),
            0
        );
        assert_eq!(
            SendMessageW(GetDlgItem(search.window, 161), BM_GETCHECK, 0, 0),
            0
        );
        assert_eq!(
            SendMessageW(GetDlgItem(search.window, 162), BM_GETCHECK, 0, 0),
            0
        );
        assert_eq!(
            SendMessageW(GetDlgItem(search.window, 163), WM_USER, 0, 0),
            22
        );
        PostMessageW(search.window, WM_COMMAND, 141, 0);
    }
    wait_until(|| restarted.0.try_wait().unwrap().is_some());
    assert!(restarted.0.wait().unwrap().success());
    let log = std::fs::read_to_string(settings.join("client.log")).unwrap();
    assert!(log.contains("UI ready") && log.contains("normal exit"));
    assert!(!log.contains("panic"));
    std::fs::remove_dir_all(temp).unwrap();
}
