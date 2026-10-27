use crate::{
    audio::{device::Audio, Mixer},
    network::{self, Bridge, UiEvent},
    settings::Settings,
};
use mini_shared::*;
use std::{
    collections::VecDeque,
    ptr::{null, null_mut},
    sync::{
        atomic::Ordering,
        mpsc::{sync_channel, Receiver},
        Arc, Mutex,
    },
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{Controls::*, WindowsAndMessaging::*},
};
const WAKE: u32 = WM_APP + 1;
const ACTION: u32 = WM_APP + 2;
const LAYOUT: u32 = WM_APP + 3;
const SERVER: i32 = 101;
const USER: i32 = 102;
const PASS: i32 = 103;
const LOGIN: i32 = 104;
const REGISTER: i32 = 105;
const LOGOUT: i32 = 106;
const FRIENDS: i32 = 110;
const NAME: i32 = 111;
const ADD: i32 = 112;
const ACCEPT: i32 = 113;
const CHAT: i32 = 114;
const CONVOS: i32 = 115;
const INVITE: i32 = 116;
const HISTORY: i32 = 120;
const OLDER: i32 = 121;
const INPUT: i32 = 122;
const SEND: i32 = 123;
const CALL: i32 = 124;
const JOIN: i32 = 125;
const DECLINE: i32 = 126;
const PEOPLE: i32 = 130;
const MIC: i32 = 131;
const DEAF: i32 = 132;
const LEAVE: i32 = 133;
const STATUS: i32 = 140;
const EXIT_APP: i32 = 141;
const PANEL: i32 = 150;
const INPUT_DEVICE: i32 = 151;
const OUTPUT_DEVICE: i32 = 152;
const MIC_GAIN: i32 = 153;
const MASTER_GAIN: i32 = 154;
const AVATARS: i32 = 155;
const UPLOAD: i32 = 156;
const REMOVE_AVATAR: i32 = 157;
const MIC_LABEL: i32 = 158;
const MASTER_LABEL: i32 = 159;
const GAIN_BASE: i32 = 170;
const LABEL_BASE: i32 = 180;
const VOICE_CLEANUP: i32 = 160;
const MIXER_CHANGE: u32 = WM_APP + 11;
fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
unsafe fn set(h: HWND, s: &str) {
    SetWindowTextW(h, w(s).as_ptr());
}
unsafe fn text(h: HWND) -> String {
    let n = GetWindowTextLengthW(h).min(8192);
    let mut b = vec![0u16; n as usize + 1];
    let n = GetWindowTextW(h, b.as_mut_ptr(), b.len() as i32);
    String::from_utf16_lossy(&b[..n as usize])
}
struct App {
    window: HWND,
    tray: crate::tray::Tray,
    rt: tokio::runtime::Runtime,
    bridge: Option<Bridge>,
    events: Receiver<UiEvent>,
    sender: std::sync::mpsc::SyncSender<UiEvent>,
    settings: Settings,
    user: Option<User>,
    state: Option<Snapshot>,
    selected: Option<i64>,
    messages: VecDeque<ChatMessage>,
    pending: Vec<(String, String, i64)>,
    failed: std::collections::HashSet<String>,
    audio: Option<Audio>,
    mixer: Arc<Mutex<Mixer>>,
    mic: bool,
    deaf: bool,
    room: Option<u64>,
    panel_open: bool,
    input_devices: Vec<String>,
    output_devices: Vec<String>,
    mixer_users: Vec<i64>,
    profile: std::rc::Rc<std::cell::RefCell<crate::profile_ui::View>>,
}
impl App {
    unsafe fn control(&self, id: i32) -> HWND {
        GetDlgItem(self.window, id)
    }
    unsafe fn status(&self, s: &str) {
        set(self.control(STATUS), s);
    }
    unsafe fn send(&self, r: Request) -> bool {
        if self
            .bridge
            .as_ref()
            .is_some_and(|b| b.requests.try_send(r).is_ok())
        {
            true
        } else {
            self.status("Not connected or action queue full");
            false
        }
    }
    unsafe fn connect(&mut self) {
        if let Some(b) = self.bridge.take() {
            b.task.abort();
        }
        self.audio = None;
        self.profile.borrow_mut().cache.clear();
        self.profile.borrow_mut().call = None;
        self.mixer_users.clear();
        self.user = None;
        self.state = None;
        self.selected = None;
        self.messages.clear();
        self.pending.clear();
        self.failed.clear();
        self.room = None;
        self.mixer.lock().unwrap().speakers.clear();
        let endpoint = text(self.control(SERVER));
        let hwnd = self.window as usize;
        self.bridge = Some(network::start(
            self.rt.handle(),
            endpoint,
            self.settings.token(),
            self.sender.clone(),
            Arc::new(move || {
                PostMessageW(hwnd as HWND, WAKE, 0, 0);
            }),
            self.mixer.clone(),
            None,
        ));
    }
    unsafe fn create(&mut self) {
        let h = self.window;
        let font = GetStockObject(DEFAULT_GUI_FONT) as usize;
        let make = |class: &str, label: &str, id: i32, style: u32| {
            let c = CreateWindowExW(
                0,
                w(class).as_ptr(),
                w(label).as_ptr(),
                WS_CHILD | WS_VISIBLE | style,
                0,
                0,
                0,
                0,
                h,
                id as usize as HMENU,
                null_mut(),
                null(),
            );
            if c.is_null() {
                crate::diagnostics::record(&format!(
                    "control creation failed: {class} id={id}: {}",
                    std::io::Error::last_os_error()
                ));
            }
            SendMessageW(c, WM_SETFONT, font, 1);
            c
        };
        make(
            "EDIT",
            &self.settings.server,
            SERVER,
            WS_BORDER | ES_AUTOHSCROLL as u32 | WS_TABSTOP,
        );
        make(
            "EDIT",
            "",
            USER,
            WS_BORDER | ES_AUTOHSCROLL as u32 | WS_TABSTOP,
        );
        make(
            "EDIT",
            "",
            PASS,
            WS_BORDER | ES_PASSWORD as u32 | ES_AUTOHSCROLL as u32 | WS_TABSTOP,
        );
        for (id, label) in [
            (LOGIN, "Log in"),
            (REGISTER, "Register"),
            (LOGOUT, "Log out"),
            (ADD, "Add friend"),
            (ACCEPT, "Accept friend"),
            (CHAT, "New chat"),
            (INVITE, "Invite to call"),
            (OLDER, "Older messages"),
            (SEND, "Send"),
            (CALL, "Call"),
            (JOIN, "Accept call"),
            (DECLINE, "Decline"),
            (MIC, "Mute mic"),
            (DEAF, "Deafen"),
            (LEAVE, "Leave"),
            (EXIT_APP, "Exit app"),
        ] {
            make("BUTTON", label, id, WS_TABSTOP);
        }
        make(
            "LISTBOX",
            "",
            FRIENDS,
            WS_BORDER | WS_VSCROLL | LBS_NOTIFY as u32 | LBS_EXTENDEDSEL as u32 | WS_TABSTOP,
        );
        make(
            "LISTBOX",
            "",
            CONVOS,
            WS_BORDER | WS_VSCROLL | LBS_NOTIFY as u32 | WS_TABSTOP,
        );
        make(
            "EDIT",
            "",
            NAME,
            WS_BORDER | ES_AUTOHSCROLL as u32 | WS_TABSTOP,
        );
        make(
            "EDIT",
            "",
            HISTORY,
            WS_BORDER
                | WS_VSCROLL
                | ES_MULTILINE as u32
                | ES_READONLY as u32
                | ES_AUTOVSCROLL as u32,
        );
        make(
            "EDIT",
            "",
            INPUT,
            WS_BORDER
                | ES_MULTILINE as u32
                | ES_WANTRETURN as u32
                | ES_AUTOVSCROLL as u32
                | WS_TABSTOP,
        );
        make("STATIC", "", PEOPLE, 0x0000000d /* SS_OWNERDRAW */);
        make(
            "STATIC",
            "Server: wss://host:4433   |   Username   |   Password (10+ characters)",
            STATUS,
            0,
        );
        make("BUTTON", "Audio / Profile", PANEL, WS_TABSTOP);
        for id in [INPUT_DEVICE, OUTPUT_DEVICE] {
            make(
                "COMBOBOX",
                "",
                id,
                CBS_DROPDOWNLIST as u32 | WS_VSCROLL | WS_TABSTOP,
            );
        }
        for (id, label) in [
            (AVATARS, "Show Avatars"),
            (VOICE_CLEANUP, "Voice cleanup"),
            (UPLOAD, "Upload avatar"),
            (REMOVE_AVATAR, "Remove avatar"),
        ] {
            make(
                "BUTTON",
                label,
                id,
                WS_TABSTOP
                    | if id == AVATARS || id == VOICE_CLEANUP {
                        BS_AUTOCHECKBOX as u32
                    } else {
                        0
                    },
            );
        }
        SendMessageW(
            self.control(AVATARS),
            BM_SETCHECK,
            if self.settings.preferences.show_avatars {
                BST_CHECKED as usize
            } else {
                BST_UNCHECKED as usize
            },
            0,
        );
        SendMessageW(
            self.control(VOICE_CLEANUP),
            BM_SETCHECK,
            if self.settings.preferences.voice_cleanup {
                BST_CHECKED as usize
            } else {
                BST_UNCHECKED as usize
            },
            0,
        );
        for id in [MIC_LABEL, MASTER_LABEL] {
            make("STATIC", "", id, 0);
        }
        for id in [MIC_GAIN, MASTER_GAIN]
            .into_iter()
            .chain(GAIN_BASE..GAIN_BASE + 7)
        {
            make("msctls_trackbar32", "", id, WS_TABSTOP | TBS_HORZ);
            SendMessageW(self.control(id), TBM_SETRANGE, 1, (200u32 << 16) as isize);
        }
        for id in LABEL_BASE..LABEL_BASE + 7 {
            make("STATIC", "", id, 0);
        }
        SendMessageW(
            self.control(MIC_GAIN),
            TBM_SETPOS,
            1,
            self.settings.preferences.microphone.min(200) as isize,
        );
        SendMessageW(
            self.control(MASTER_GAIN),
            TBM_SETPOS,
            1,
            self.settings.preferences.master.min(200) as isize,
        );
        if let Err(error) = crate::message_input::attach(self.control(INPUT), h) {
            self.status(&error.to_string());
        }
        SendMessageW(self.control(INPUT), EM_SETLIMITTEXT, MAX_TEXT, 0);
        SendMessageW(self.control(USER), EM_SETLIMITTEXT, 24, 0);
        SendMessageW(self.control(PASS), EM_SETLIMITTEXT, 128, 0);
        self.layout();
        if !self.settings.server.is_empty() {
            self.connect();
        }
    }
    unsafe fn layout(&self) {
        let mut r = std::mem::zeroed();
        GetClientRect(self.window, &mut r);
        let width = r.right.max(800);
        let height = r.bottom.max(600);
        let pos = |id, x, y, ww, hh| {
            MoveWindow(self.control(id), x, y, ww, hh, 1);
        };
        pos(SERVER, 8, 8, 250, 26);
        pos(USER, 266, 8, 125, 26);
        pos(PASS, 399, 8, 150, 26);
        pos(LOGIN, 557, 8, 75, 26);
        pos(REGISTER, 640, 8, 80, 26);
        pos(LOGOUT, width - 80, 8, 72, 26);
        pos(STATUS, 8, 42, width - 16, 32);
        pos(NAME, 8, 82, 190, 25);
        pos(ADD, 8, 112, 90, 26);
        pos(ACCEPT, 104, 112, 94, 26);
        pos(FRIENDS, 8, 144, 190, (height - 360) / 2);
        pos(CHAT, 8, (height - 360) / 2 + 150, 90, 26);
        pos(INVITE, 104, (height - 360) / 2 + 150, 94, 26);
        pos(
            CONVOS,
            8,
            (height - 360) / 2 + 184,
            190,
            height - ((height - 360) / 2 + 192),
        );
        pos(OLDER, 208, 82, 120, 26);
        pos(CALL, 336, 82, 75, 26);
        pos(JOIN, 419, 82, 100, 26);
        pos(DECLINE, 527, 82, 80, 26);
        let avatar_extra = if self.settings.preferences.show_avatars {
            16
        } else {
            0
        };
        let panel_height = if self.panel_open {
            108 + self.mixer_users.len() as i32 * 24
        } else {
            0
        };
        pos(
            HISTORY,
            208,
            116,
            width - 216,
            (height - 284 - avatar_extra - panel_height).max(40),
        );
        let y = height - 168 - avatar_extra - panel_height;
        pos(INPUT_DEVICE, 208, y, 270, 260);
        pos(OUTPUT_DEVICE, 486, y, width - 494, 260);
        pos(MIC_LABEL, 208, y + 30, 130, 22);
        pos(MIC_GAIN, 340, y + 26, 138, 28);
        pos(MASTER_LABEL, 486, y + 30, 130, 22);
        pos(MASTER_GAIN, 618, y + 26, (width - 626).max(90), 28);
        pos(VOICE_CLEANUP, 612, y + 60, 160, 26);
        pos(AVATARS, 208, y + 60, 140, 26);
        pos(UPLOAD, 356, y + 60, 120, 26);
        pos(REMOVE_AVATAR, 484, y + 60, 120, 26);
        for i in 0..7 {
            pos(LABEL_BASE + i, 208, y + 100 + i * 24, 180, 22);
            pos(GAIN_BASE + i, 396, y + 96 + i * 24, width - 404, 26);
        }
        for id in [
            INPUT_DEVICE,
            OUTPUT_DEVICE,
            MIC_LABEL,
            MIC_GAIN,
            MASTER_LABEL,
            MASTER_GAIN,
            AVATARS,
            VOICE_CLEANUP,
            UPLOAD,
            REMOVE_AVATAR,
        ] {
            ShowWindow(
                self.control(id),
                if self.panel_open { SW_SHOW } else { SW_HIDE },
            );
        }
        for i in 0..7 {
            for id in [GAIN_BASE + i, LABEL_BASE + i] {
                ShowWindow(
                    self.control(id),
                    if self.panel_open && (i as usize) < self.mixer_users.len() {
                        SW_SHOW
                    } else {
                        SW_HIDE
                    },
                );
            }
        }
        set(
            self.control(MIC_LABEL),
            &format!("Mic: {}%", self.settings.preferences.microphone.min(200)),
        );
        set(
            self.control(MASTER_LABEL),
            &format!("Output: {}%", self.settings.preferences.master.min(200)),
        );
        pos(INPUT, 208, height - 160 - avatar_extra, width - 296, 28);
        pos(SEND, width - 80, height - 160 - avatar_extra, 72, 28);
        pos(
            PEOPLE,
            208,
            height - 124 - avatar_extra,
            width - 216,
            70 + avatar_extra,
        );
        pos(MIC, 208, height - 42, 105, 30);
        pos(DEAF, 321, height - 42, 105, 30);
        pos(LEAVE, 434, height - 42, 85, 30);
        pos(EXIT_APP, width - 100, height - 42, 92, 30);
        pos(PANEL, 527, height - 42, 130, 30);
        for id in [PEOPLE, MIC, DEAF, LEAVE] {
            ShowWindow(
                self.control(id),
                if self.room.is_some() {
                    SW_SHOW
                } else {
                    SW_HIDE
                },
            );
        }
        let logged = self.user.is_some();
        EnableWindow(self.control(SERVER), if logged { 0 } else { 1 });
        for id in [USER, PASS, LOGIN, REGISTER] {
            ShowWindow(self.control(id), if logged { SW_HIDE } else { SW_SHOW });
        }
        ShowWindow(self.control(LOGOUT), if logged { SW_SHOW } else { SW_HIDE });
        let incoming = self.incoming_call().is_some();
        for id in [JOIN, DECLINE] {
            ShowWindow(self.control(id), if incoming { SW_SHOW } else { SW_HIDE });
        }
    }
    fn incoming_call(&self) -> Option<u64> {
        let u = self.user.as_ref()?.id;
        self.state
            .as_ref()?
            .calls
            .iter()
            .find(|c| c.invited.contains(&u))
            .map(|c| c.id)
    }
    unsafe fn chosen_friends(&self) -> Vec<i64> {
        let mut ids = [0i32; 128];
        let n = SendMessageW(
            self.control(FRIENDS),
            LB_GETSELITEMS,
            128,
            ids.as_mut_ptr() as isize,
        )
        .max(0) as usize;
        let Some(s) = &self.state else {
            return vec![];
        };
        let all: Vec<_> = s.friends.iter().chain(s.incoming.iter()).collect();
        ids[..n]
            .iter()
            .filter_map(|i| all.get(*i as usize).map(|u| u.id))
            .collect()
    }
    unsafe fn render_messages(&self) {
        let mut out = String::new();
        for m in &self.messages {
            out.push_str(&format!(
                "{}  {}\r\n{}\r\n\r\n",
                stamp(m.timestamp),
                m.sender.username,
                m.text.replace("\r\n", "\n").replace('\n', "\r\n")
            ));
        }
        for (nonce, t, conversation) in &self.pending {
            if Some(*conversation) == self.selected {
                out.push_str(&format!(
                    "[{}] {t}\r\n",
                    if self.failed.contains(nonce) {
                        "not sent — copy to retry"
                    } else {
                        "sending / awaiting acknowledgement"
                    }
                ));
            }
        }
        set(self.control(HISTORY), &out);
    }
    unsafe fn update(&mut self, chosen: Vec<i64>) {
        let Some(s) = &self.state else {
            return;
        };
        let old = self.selected;
        SendMessageW(self.control(FRIENDS), LB_RESETCONTENT, 0, 0);
        for (index, (u, incoming)) in s
            .friends
            .iter()
            .map(|x| (x, false))
            .chain(s.incoming.iter().map(|x| (x, true)))
            .enumerate()
        {
            let label = format!("{}{}", if incoming { "Request: " } else { "" }, u.username);
            SendMessageW(
                self.control(FRIENDS),
                LB_ADDSTRING,
                0,
                w(&label).as_ptr() as isize,
            );
            if chosen.contains(&u.id) {
                SendMessageW(self.control(FRIENDS), LB_SETSEL, 1, index as isize);
            }
        }
        SendMessageW(self.control(CONVOS), LB_RESETCONTENT, 0, 0);
        if self.selected.is_none() {
            self.selected = s.conversations.first().map(|c| c.id);
        }
        for (i, c) in s.conversations.iter().enumerate() {
            let label = c
                .users
                .iter()
                .filter(|u| self.user.as_ref().is_none_or(|me| me.id != u.id))
                .map(|u| u.username.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            SendMessageW(
                self.control(CONVOS),
                LB_ADDSTRING,
                0,
                w(&label).as_ptr() as isize,
            );
            if Some(c.id) == self.selected {
                SendMessageW(self.control(CONVOS), LB_SETCURSEL, i, 0);
            }
        }
        let current = self.user.as_ref().and_then(|u| {
            s.calls
                .iter()
                .find(|c| c.participants.iter().any(|p| p.user.id == u.id))
        });
        let room = current.map(|c| c.id);
        if let Some(p) = current.and_then(|c| {
            c.participants
                .iter()
                .find(|p| self.user.as_ref().is_some_and(|u| u.id == p.user.id))
        }) {
            self.mic = p.mic_muted;
            self.deaf = p.deafened;
        }
        if let Some(a) = &self.audio {
            a.mic.store(self.mic, Ordering::SeqCst);
            a.deaf.store(self.deaf, Ordering::SeqCst);
        }

        let mut people = String::new();
        if let Some(c) = current {
            for p in &c.participants {
                people.push_str(&format!(
                    "{}{}{}   ",
                    p.user.username,
                    if p.labeled { " — gay" } else { "" },
                    if p.mic_muted { " [muted]" } else { "" }
                ));
            }
        }
        set(self.control(PEOPLE), &people);
        let participants = current.map(|c| c.participants.clone()).unwrap_or_default();
        self.mixer_users = participants
            .iter()
            .filter(|p| self.user.as_ref().is_some_and(|me| me.id != p.user.id))
            .map(|p| p.user.id)
            .collect();
        {
            let mut mixer = self.mixer.lock().unwrap();
            mixer.master = self.settings.preferences.master.min(200) as f32 / 100.;
            mixer.gains.clear();
            for (i, p) in participants
                .iter()
                .filter(|p| self.user.as_ref().is_some_and(|me| me.id != p.user.id))
                .enumerate()
            {
                let gain = self
                    .settings
                    .preferences
                    .user_gain(&self.settings.server, p.user.id);
                mixer.set_user_gain(p.user.id, gain as f32 / 100.);
                set(
                    self.control(LABEL_BASE + i as i32),
                    &format!("{}: {gain}%", p.user.username),
                );
                SendMessageW(
                    self.control(GAIN_BASE + i as i32),
                    TBM_SETPOS,
                    1,
                    gain as isize,
                );
            }
        }
        let avatar_users = participants.into_iter().map(|p| p.user).collect();
        InvalidateRect(self.control(PEOPLE), null(), 0);
        if room != self.room {
            self.audio = None;
            self.mixer.lock().unwrap().speakers.clear();

            self.room = room;
            if room.is_some() {
                if let Some(b) = &self.bridge {
                    match Audio::open(
                        b.audio.clone(),
                        self.mixer.clone(),
                        &self.settings.preferences,
                        self.mic,
                        self.deaf,
                    ) {
                        Ok(a) => {
                            a.mic.store(self.mic, Ordering::SeqCst);
                            a.deaf.store(self.deaf, Ordering::SeqCst);
                            self.audio = Some(a);
                        }
                        Err(e) => {
                            self.status(&format!("Audio unavailable: {e}"));
                            self.send(Request::Leave {});
                        }
                    }
                }
            }
        }
        self.sync_avatars(avatar_users);
        set(
            self.control(MIC),
            if self.mic { "Unmute mic" } else { "Mute mic" },
        );
        set(
            self.control(DEAF),
            if self.deaf { "Undeafen" } else { "Deafen" },
        );
        self.layout();
        if old != self.selected {
            self.messages.clear();
            self.render_messages();
            if let Some(c) = self.selected {
                self.send(Request::History {
                    conversation: c,
                    before: None,
                });
            }
        }
    }
    unsafe fn command(&mut self, id: i32, notification: u16) {
        if id == EXIT_APP {
            PostMessageW(self.window, crate::tray::EXIT, 0, 0);
            return;
        }
        match id {
            PANEL => {
                self.panel_open = !self.panel_open;
                if self.panel_open {
                    self.enumerate_devices();
                }
                self.layout();
            }
            INPUT_DEVICE | OUTPUT_DEVICE if notification == CBN_SELCHANGE as u16 => {
                let index = SendMessageW(self.control(id), CB_GETCURSEL, 0, 0);
                let list = if id == INPUT_DEVICE {
                    &self.input_devices
                } else {
                    &self.output_devices
                };
                let selection = if index <= 0 {
                    None
                } else {
                    list.get(index as usize - 1).cloned()
                };
                if id == INPUT_DEVICE {
                    self.settings.preferences.input_device = selection;
                } else {
                    self.settings.preferences.output_device = selection;
                }
                self.save_preferences();
                self.reopen_audio();
            }
            VOICE_CLEANUP => {
                let enabled = SendMessageW(self.control(VOICE_CLEANUP), BM_GETCHECK, 0, 0)
                    == BST_CHECKED as isize;
                self.settings.preferences.voice_cleanup = enabled;
                if let Some(audio) = &self.audio {
                    audio.cleanup.store(enabled, Ordering::Relaxed);
                }
                self.save_preferences();
            }
            AVATARS => {
                self.settings.preferences.show_avatars =
                    SendMessageW(self.control(AVATARS), BM_GETCHECK, 0, 0) == BST_CHECKED as isize;
                self.save_preferences();
                let users = self.call_users();
                self.sync_avatars(users);
                self.layout();
                InvalidateRect(self.control(PEOPLE), null(), 0);
            }
            UPLOAD => {
                if self.user.is_some() {
                    PostMessageW(self.window, crate::profile_ui::PICK, 0, 0);
                } else {
                    self.status("Sign in before uploading an avatar");
                }
            }
            REMOVE_AVATAR => {
                self.send(Request::AvatarRemove {});
            }
            LOGIN | REGISTER => {
                if self.bridge.is_none() || text(self.control(SERVER)) != self.settings.server {
                    let server = text(self.control(SERVER));
                    let _ = self.settings.save(server, None);
                    self.connect();
                }
                let username = text(self.control(USER));
                let password = text(self.control(PASS));
                let req = if id == LOGIN {
                    Request::Login { username, password }
                } else {
                    Request::Register { username, password }
                };
                self.send(req);
                set(self.control(PASS), "");
            }
            LOGOUT => {
                self.send(Request::Logout {});
                let _ = self.settings.save(text(self.control(SERVER)), None);
                self.audio = None;
            }
            ADD => {
                self.send(Request::FriendRequest {
                    username: text(self.control(NAME)),
                });
            }
            ACCEPT => {
                for user in self.chosen_friends() {
                    self.send(Request::FriendAccept { user });
                }
            }
            CHAT => {
                let users = self.chosen_friends();
                if !users.is_empty() {
                    self.send(Request::Conversation { users });
                }
            }
            INVITE => {
                if let Some(call) = self.room {
                    for user in self.chosen_friends() {
                        self.send(Request::Invite { call, user });
                    }
                }
            }
            CONVOS if notification == LBN_SELCHANGE as u16 => {
                let i = SendMessageW(self.control(CONVOS), LB_GETCURSEL, 0, 0);
                if i >= 0 {
                    if let Some(c) = self
                        .state
                        .as_ref()
                        .and_then(|s| s.conversations.get(i as usize))
                    {
                        self.selected = Some(c.id);
                        self.messages.clear();
                        self.render_messages();
                        self.send(Request::History {
                            conversation: c.id,
                            before: None,
                        });
                    }
                }
            }
            SEND => {
                if let Some(conversation) = self.selected {
                    let text = text(self.control(INPUT)).replace("\r\n", "\n");
                    if text.trim().is_empty() || text.len() > MAX_TEXT {
                        self.status("Message must be 1–4096 UTF-8 bytes");
                        return;
                    }
                    let nonce = hex::encode(rand::random::<[u8; 16]>());
                    self.pending.retain(|(nonce, value, c)| {
                        let retry =
                            *c == conversation && *value == text && self.failed.contains(nonce);
                        if retry {
                            self.failed.remove(nonce);
                        }
                        !retry
                    });
                    if self.pending.len() >= 32 {
                        self.status("Outbox full; message remains in input");
                        return;
                    }
                    if self.send(Request::Send {
                        conversation,
                        nonce: nonce.clone(),
                        text: text.clone(),
                    }) {
                        if self.pending.len() < 32 {
                            self.pending.push((nonce, text, conversation));
                        }
                        set(self.control(INPUT), "");
                        self.render_messages();
                        SetFocus(self.control(INPUT));
                    }
                }
            }
            OLDER => {
                if let Some(conversation) = self.selected {
                    self.send(Request::History {
                        conversation,
                        before: self.messages.front().map(|m| m.id),
                    });
                }
            }
            CALL => {
                if let Some(conversation) = self.selected {
                    self.send(Request::Call { conversation });
                }
            }
            JOIN => {
                if let Some(call) = self.incoming_call() {
                    self.send(Request::Join { call });
                }
            }
            DECLINE => {
                if let Some(call) = self.incoming_call() {
                    self.send(Request::Decline { call });
                }
            }
            LEAVE => {
                self.audio = None;
                self.send(Request::Leave {});
            }
            MIC | DEAF if self.room.is_some() => {
                if id == MIC {
                    self.mic = !self.mic;
                } else {
                    self.deaf = !self.deaf;
                }
                if let Some(a) = &self.audio {
                    a.mic.store(self.mic, Ordering::SeqCst);
                    a.deaf.store(self.deaf, Ordering::SeqCst);
                    if self.deaf {
                        self.mixer.lock().unwrap().speakers.clear();
                    }
                }
                self.send(Request::Controls {
                    mic_muted: self.mic,
                    deafened: self.deaf,
                });
                set(
                    self.control(MIC),
                    if self.mic { "Unmute mic" } else { "Mute mic" },
                );
                set(
                    self.control(DEAF),
                    if self.deaf { "Undeafen" } else { "Deafen" },
                );
            }
            _ => {}
        }
    }
    unsafe fn events(&mut self) {
        while let Ok(e) = self.events.try_recv() {
            match e {
                UiEvent::Status(s) => {
                    if s == "Online" {
                        self.profile.borrow_mut().cache.retry();
                    }
                    if s.starts_with("Disconnected") {
                        self.profile.borrow_mut().cache.clear();
                        self.profile.borrow_mut().call = None;
                        self.mixer_users.clear();
                        self.audio = None;
                        self.room = None;
                        self.state = None;
                        self.layout();
                    }
                    self.status(&s);
                }
                UiEvent::Server(e) => match e {
                    Event::Avatar {
                        user,
                        revision,
                        data,
                    } => {
                        if let Err(error) = self.profile.borrow_mut().cache.accept(
                            self.settings.preferences.show_avatars,
                            user,
                            revision.as_deref(),
                            &data,
                        ) {
                            self.status(&error.to_string());
                        }
                        InvalidateRect(self.control(PEOPLE), null(), 0);
                    }
                    Event::AvatarSaved { .. } => self.status("Avatar updated"),
                    Event::Auth { user, token, .. } => {
                        set(self.window, &format!("Mini Voice — {}", user.username));
                        self.user = Some(user);
                        if let Some(conversation) = self.selected {
                            self.send(Request::History {
                                conversation,
                                before: None,
                            });
                        }
                        // Bind credentials to the origin actually connected, even if the
                        // address field was edited while authentication was in flight.
                        if let Some(bridge) = &self.bridge {
                            let server = bridge.endpoint.clone();
                            set(self.control(SERVER), &server);
                            if let Err(e) = self.settings.save(server, Some(&token)) {
                                self.status(&e.to_string());
                            }
                        }
                        self.layout();
                    }
                    Event::State { state } => {
                        let chosen = self.chosen_friends();
                        self.state = Some(state);
                        self.update(chosen);
                    }
                    Event::History {
                        conversation,
                        messages,
                    } if Some(conversation) == self.selected => {
                        let older = self
                            .messages
                            .front()
                            .is_some_and(|m| messages.last().is_some_and(|n| n.id < m.id));
                        for m in messages {
                            if !self.messages.iter().any(|n| n.id == m.id) {
                                self.messages.push_back(m);
                            }
                        }
                        self.messages.make_contiguous().sort_by_key(|m| m.id);
                        while self.messages.len() > 200 {
                            if older {
                                self.messages.pop_back();
                            } else {
                                self.messages.pop_front();
                            }
                        }
                        self.render_messages();
                    }
                    Event::Message { message } => {
                        if Some(message.conversation) == self.selected
                            && !self.messages.iter().any(|m| m.id == message.id)
                        {
                            self.pending.retain(|(n, _, _)| n != &message.nonce);
                            self.messages.push_back(message);
                            while self.messages.len() > 200 {
                                self.messages.pop_front();
                            }
                            self.render_messages();
                        }
                    }
                    Event::Ack { nonce, .. } => {
                        self.pending.retain(|(n, _, _)| n != &nonce);
                        self.render_messages();
                        self.status("Delivered");
                    }
                    Event::Rejected { nonce, message } => {
                        if let Some((_, value, conversation)) =
                            self.pending.iter().find(|(n, _, _)| n == &nonce)
                        {
                            if Some(*conversation) == self.selected
                                && text(self.control(INPUT)).is_empty()
                            {
                                set(self.control(INPUT), &value.replace('\n', "\r\n"));
                                self.pending.retain(|(n, _, _)| n != &nonce);
                            } else {
                                self.failed.insert(nonce.clone());
                            }
                        }
                        self.render_messages();
                        self.status(&format!("Not sent: {message}"));
                    }
                    Event::Error { message } => self.status(&message),
                    Event::LoggedOut => {
                        self.audio = None;
                        self.profile.borrow_mut().cache.clear();
                        self.profile.borrow_mut().call = None;
                        self.mixer_users.clear();
                        self.user = None;
                        set(self.window, "Mini Voice");
                        self.state = None;
                        self.selected = None;
                        self.room = None;
                        self.messages.clear();
                        self.pending.clear();
                        self.failed.clear();
                        let _ = self.settings.save(text(self.control(SERVER)), None);
                        self.render_messages();
                        SendMessageW(self.control(FRIENDS), LB_RESETCONTENT, 0, 0);
                        SendMessageW(self.control(CONVOS), LB_RESETCONTENT, 0, 0);
                        self.layout();
                    }
                    _ => {}
                },
            }
        }
    }
}
impl App {
    unsafe fn save_preferences(&self) {
        if let Err(e) = self.settings.persist() {
            self.status(&format!("Settings not saved: {e}"));
        }
    }
    unsafe fn enumerate_devices(&mut self) {
        self.input_devices = crate::audio::device::devices(true);
        self.output_devices = crate::audio::device::devices(false);
        if self.input_devices.is_empty() && self.output_devices.is_empty() {
            self.status(
                "No audio devices detected; check Windows audio service and connected devices",
            );
        }
        for (id, list, selected) in [
            (
                INPUT_DEVICE,
                &mut self.input_devices,
                &self.settings.preferences.input_device,
            ),
            (
                OUTPUT_DEVICE,
                &mut self.output_devices,
                &self.settings.preferences.output_device,
            ),
        ] {
            if let Some(name) = selected {
                if !list.contains(name) {
                    list.push(name.clone());
                }
            }
            let control = GetDlgItem(self.window, id);
            SendMessageW(control, CB_RESETCONTENT, 0, 0);
            SendMessageW(
                control,
                CB_ADDSTRING,
                0,
                w(if id == INPUT_DEVICE {
                    "Microphone: Default Windows Device"
                } else {
                    "Output: Default Windows Device"
                })
                .as_ptr() as isize,
            );
            for name in list.iter() {
                SendMessageW(control, CB_ADDSTRING, 0, w(name).as_ptr() as isize);
            }
            let index = selected
                .as_ref()
                .and_then(|s| list.iter().position(|n| n == s))
                .map(|i| i + 1)
                .unwrap_or(0);
            SendMessageW(control, CB_SETCURSEL, index, 0);
        }
    }
    unsafe fn reopen_audio(&mut self) {
        if self.room.is_none() {
            return;
        }
        // Drop both old streams before opening replacements: never duplicate capture.
        self.audio = None;
        if let Some(bridge) = &self.bridge {
            match Audio::open(
                bridge.audio.clone(),
                self.mixer.clone(),
                &self.settings.preferences,
                self.mic,
                self.deaf,
            ) {
                Ok(audio) => {
                    audio.mic.store(self.mic, Ordering::Relaxed);
                    audio.deaf.store(self.deaf, Ordering::Relaxed);
                    self.audio = Some(audio);
                    self.status(
                        "Audio device applied (unavailable selections use Windows default)",
                    );
                }
                Err(error) => {
                    self.status(&format!(
                        "Audio unavailable: {error}; select another device"
                    ));
                }
            }
        }
    }
    unsafe fn mixer_change(&mut self, id: i32, notification: u16) {
        let value =
            SendMessageW(self.control(id), WM_USER /* TBM_GETPOS */, 0, 0).clamp(0, 200) as u16;
        if id == MIC_GAIN {
            self.settings.preferences.microphone = value;
            if let Some(a) = &self.audio {
                a.input_gain.store(value as u32, Ordering::Relaxed);
            }
        } else if id == MASTER_GAIN {
            self.settings.preferences.master = value;
            self.mixer.lock().unwrap().master = value as f32 / 100.;
        } else if let Some(user) = id
            .checked_sub(GAIN_BASE)
            .and_then(|i| self.mixer_users.get(i as usize))
            .copied()
        {
            self.settings
                .preferences
                .set_user_gain(&self.settings.server, user, value);
            self.mixer
                .lock()
                .unwrap()
                .set_user_gain(user, value as f32 / 100.);
            let name = self
                .state
                .as_ref()
                .and_then(|s| s.calls.iter().find(|c| Some(c.id) == self.room))
                .and_then(|c| c.participants.iter().find(|p| p.user.id == user))
                .map(|p| p.user.username.as_str())
                .unwrap_or("User");
            set(
                self.control(LABEL_BASE + id - GAIN_BASE),
                &format!("{name}: {value}%"),
            );
        }
        set(
            self.control(MIC_LABEL),
            &format!("Mic: {}%", self.settings.preferences.microphone),
        );
        set(
            self.control(MASTER_LABEL),
            &format!("Output: {}%", self.settings.preferences.master),
        );
        if notification != TB_THUMBTRACK as u16 {
            self.save_preferences();
        }
    }
    fn call_users(&self) -> Vec<User> {
        self.state
            .as_ref()
            .and_then(|s| s.calls.iter().find(|c| Some(c.id) == self.room))
            .map(|c| c.participants.iter().map(|p| p.user.clone()).collect())
            .unwrap_or_default()
    }
    unsafe fn sync_avatars(&mut self, users: Vec<User>) {
        let requests = {
            let mut view = self.profile.borrow_mut();
            view.enabled = self.settings.preferences.show_avatars;
            view.call = self
                .state
                .as_ref()
                .and_then(|s| s.calls.iter().find(|c| Some(c.id) == self.room))
                .cloned();
            let enabled = view.enabled;
            view.cache.reconcile(enabled, users)
        };
        for user in requests {
            if !self.send(Request::AvatarGet { user }) {
                self.profile.borrow_mut().cache.failed(user);
            }
        }
    }
}
fn stamp(t: u64) -> String {
    let days = (t / 86400) as i64;
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    y += if m <= 2 { 1 } else { 0 };
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} UTC",
        t / 3600 % 24,
        t / 60 % 60,
        t % 60
    )
}
unsafe extern "system" fn wndproc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_GETMINMAXINFO {
        let info = &mut *(lp as *mut MINMAXINFO);
        info.ptMinTrackSize.x = 820;
        info.ptMinTrackSize.y = 656;
        return 0;
    }
    // Queue control notifications so synchronous Win32 callbacks cannot reborrow App.
    if msg == WM_HSCROLL {
        PostMessageW(h, MIXER_CHANGE, wp, lp);
        return 0;
    }
    if msg == WM_COMMAND {
        PostMessageW(h, ACTION, wp, lp);
        return 0;
    }
    if msg == WM_SIZE {
        if wp == SIZE_MINIMIZED as usize {
            PostMessageW(h, WM_CLOSE, 0, 0);
        }
        PostMessageW(h, LAYOUT, 0, 0);
        return 0;
    }
    if matches!(
        msg,
        WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX
    ) {
        SetTextColor(wp as HDC, 0x00dedede);
        SetBkColor(wp as HDC, 0x00202020);
        SetDCBrushColor(wp as HDC, 0x00202020);
        return GetStockObject(DC_BRUSH) as isize;
    }
    if msg == crate::tray::CALLBACK {
        crate::tray::callback(h, lp as u32);
        return 0;
    }
    if msg == crate::tray::SHOW {
        crate::tray::show(h);
        return 0;
    }
    if msg == WM_DRAWITEM && lp != 0 {
        let draw = &*(lp as *const DRAWITEMSTRUCT);
        if draw.CtlID == PEOPLE as u32 {
            let cell = GetPropW(h, crate::profile_ui::property().as_ptr())
                as *const std::cell::RefCell<crate::profile_ui::View>;
            if !cell.is_null() {
                if let Ok(view) = (&*cell).try_borrow() {
                    crate::profile_ui::draw(
                        draw.hDC,
                        draw.rcItem,
                        view.call.as_ref(),
                        view.enabled,
                        &view.cache,
                    );
                }
            }
            return 1;
        }
    }
    if msg == crate::profile_ui::PICK {
        // The native picker runs a modal loop. Hold no App borrow across it.
        let result = crate::profile_ui::choose(h);
        let ptr = GetWindowLongPtrW(h, GWLP_USERDATA) as *mut App;
        if !ptr.is_null() {
            let a = &mut *ptr;
            match result {
                Ok(Some(bytes)) => {
                    if a.bridge
                        .as_ref()
                        .is_some_and(|b| b.avatars.try_send(bytes).is_ok())
                    {
                        a.status("Uploading avatar…");
                    } else {
                        a.status("Avatar upload busy or offline; select the file again");
                    }
                }
                Ok(None) => {}
                Err(e) => a.status(&e.to_string()),
            }
        }
        return 0;
    }
    let ptr = GetWindowLongPtrW(h, GWLP_USERDATA) as *mut App;
    if !ptr.is_null() {
        static TASKBAR_CREATED: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
        let taskbar_created =
            *TASKBAR_CREATED.get_or_init(|| RegisterWindowMessageW(w("TaskbarCreated").as_ptr()));
        if taskbar_created != 0 && msg == taskbar_created {
            (&*ptr).tray.explorer_restarted();
            return 0;
        }
        match msg {
            MIXER_CHANGE => {
                (&mut *ptr).mixer_change(GetDlgCtrlID(lp as HWND), wp as u16);
                return 0;
            }
            ACTION => {
                if ((wp >> 16) as u16) == BN_CLICKED as u16
                    || ([INPUT_DEVICE, OUTPUT_DEVICE].contains(&((wp & 0xffff) as i32))
                        && ((wp >> 16) as u16) == CBN_SELCHANGE as u16)
                    || ((wp & 0xffff) as i32 == CONVOS
                        && ((wp >> 16) as u16) == LBN_SELCHANGE as u16)
                {
                    (&mut *ptr).command((wp & 0xffff) as i32, (wp >> 16) as u16);
                }
                return 0;
            }
            WAKE => {
                (&mut *ptr).events();
                return 0;
            }
            LAYOUT => {
                (&*ptr).layout();
                return 0;
            }
            WM_TIMER => {
                let a = &mut *ptr;
                a.tray
                    .update(&text(a.control(STATUS)), a.room.is_some(), a.mic);
                if a.audio
                    .as_ref()
                    .is_some_and(|x| x.failed.load(Ordering::Relaxed))
                {
                    a.audio = None;
                    a.send(Request::Leave {});
                    a.status(
                        "Audio device disconnected; call ended. Reconnect device and call again.",
                    );
                }
                return 0;
            }
            WM_CLOSE => {
                let a = &*ptr;
                a.tray
                    .hide(&text(a.control(STATUS)), a.room.is_some(), a.mic);
                return 0;
            }
            crate::tray::EXIT => {
                let a = &mut *ptr;
                a.tray.remove();
                if let Some(b) = a.bridge.take() {
                    b.task.abort();
                }
                a.audio = None;
                DestroyWindow(h);
                return 0;
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                return 0;
            }
            _ => {}
        }
    }
    DefWindowProcW(h, msg, wp, lp)
}
pub fn run() -> anyhow::Result<()> {
    crate::diagnostics::record("initializing Windows UI");
    unsafe {
        let controls = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_BAR_CLASSES,
        };
        InitCommonControlsEx(&controls);
        let instance = GetModuleHandleW(null());
        let class = w("MiniVoiceWindow");
        let brush = CreateSolidBrush(0x00202020);
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            // Win32 MAKEINTRESOURCEW(1): an integer resource ID, not a pointer.
            hIcon: LoadIconW(instance, std::ptr::without_provenance(1)),
            hbrBackground: brush,
            ..std::mem::zeroed()
        };
        anyhow::ensure!(
            RegisterClassW(&wc) != 0,
            "Window class registration failed: {}",
            std::io::Error::last_os_error()
        );
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            w("Mini Voice").as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            940,
            700,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        anyhow::ensure!(
            !hwnd.is_null(),
            "Window creation failed: {}",
            std::io::Error::last_os_error()
        );
        let (tx, rx) = sync_channel(128);
        let mut app = Box::new(App {
            window: hwnd,
            tray: crate::tray::Tray::new(hwnd),
            rt: tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .max_blocking_threads(2)
                .enable_all()
                .build()?,
            bridge: None,
            events: rx,
            sender: tx,
            settings: Settings::load(),
            user: None,
            state: None,
            selected: None,
            messages: VecDeque::new(),
            pending: vec![],
            failed: std::collections::HashSet::new(),
            audio: None,
            mixer: Arc::new(Mutex::new(Mixer::new())),
            mic: false,
            deaf: false,
            room: None,
            panel_open: false,
            input_devices: vec![],
            output_devices: vec![],
            mixer_users: vec![],
            profile: std::rc::Rc::new(std::cell::RefCell::new(crate::profile_ui::View::default())),
        });
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, (&mut *app as *mut App) as isize);
        SetPropW(
            hwnd,
            crate::profile_ui::property().as_ptr(),
            std::rc::Rc::as_ptr(&app.profile) as HANDLE,
        );
        app.profile.borrow_mut().enabled = app.settings.preferences.show_avatars;
        app.create();
        SetTimer(hwnd, 1, 1000, None);
        ShowWindow(hwnd, SW_SHOW);
        app.tray.update(
            "Ready; close window to keep running in background",
            false,
            false,
        );
        crate::diagnostics::record("UI ready");
        let mut msg = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            if msg.message == WM_SYSKEYDOWN
                && msg.wParam == b'V' as usize
                && (msg.lParam & (1 << 30)) == 0
            {
                if app.room.is_some() {
                    app.send(Request::Toggle {});
                }
                continue;
            }
            if msg.message == WM_SYSKEYDOWN && msg.wParam == 0x73 {
                PostMessageW(hwnd, WM_CLOSE, 0, 0);
                continue;
            }
            if IsDialogMessageW(hwnd, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        RemovePropW(hwnd, crate::profile_ui::property().as_ptr());
        drop(app);
        DeleteObject(brush);
        crate::diagnostics::record("normal exit");
    }
    Ok(())
}
