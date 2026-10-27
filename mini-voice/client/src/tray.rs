use std::{
    cell::{Cell, RefCell},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{Shell::*, WindowsAndMessaging::*},
};

pub const CALLBACK: u32 = WM_APP + 4;
pub const SHOW: u32 = WM_APP + 5;
pub const EXIT: u32 = WM_APP + 6;
const OPEN_ITEM: usize = 40001;
const EXIT_ITEM: usize = 40002;
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

pub struct Tray {
    window: HWND,
    present: Cell<bool>,
    last_tip: RefCell<String>,
}
impl Tray {
    pub fn new(window: HWND) -> Self {
        Self {
            window,
            present: Cell::new(false),
            last_tip: RefCell::new(String::new()),
        }
    }
    unsafe fn data(&self) -> NOTIFYICONDATAW {
        let mut data: NOTIFYICONDATAW = std::mem::zeroed();
        data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = self.window;
        data.uID = 1;
        data.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        data.uCallbackMessage = CALLBACK;
        data.hIcon = LoadIconW(GetModuleHandleW(null()), std::ptr::without_provenance(1));
        if data.hIcon.is_null() {
            data.hIcon = LoadIconW(null_mut(), IDI_APPLICATION);
        }
        data
    }
    pub unsafe fn update(&self, status: &str, in_call: bool, muted: bool) -> bool {
        let mode = if IsWindowVisible(self.window) == 0 {
            "Background"
        } else {
            "Open"
        };
        let call = if in_call {
            if muted {
                " | Call: mic muted"
            } else {
                " | In call"
            }
        } else {
            ""
        };
        let tip = format!("goyvoice | {mode}{call}\n{status}");
        if self.present.get() && *self.last_tip.borrow() == tip {
            return true;
        }
        let mut data = self.data();
        // Leave a terminating NUL; do not split a surrogate pair.
        let mut text: Vec<u16> = tip.encode_utf16().take(data.szTip.len() - 1).collect();
        if text.last().is_some_and(|c| (0xd800..=0xdbff).contains(c)) {
            text.pop();
        }
        data.szTip[..text.len()].copy_from_slice(&text);
        let mut ok = self.present.get() && Shell_NotifyIconW(NIM_MODIFY, &data) != 0;
        if !ok {
            ok = Shell_NotifyIconW(NIM_ADD, &data) != 0;
        }
        self.present.set(ok);
        if ok {
            *self.last_tip.borrow_mut() = tip;
        }
        ok
    }
    pub fn explorer_restarted(&self) {
        self.present.set(false);
    }
    pub unsafe fn hide(&self, status: &str, in_call: bool, muted: bool) {
        // Never hide the only window if Windows cannot create the tray icon.
        if self.update(status, in_call, muted) {
            ShowWindow(self.window, SW_HIDE);
            self.update(status, in_call, muted);
        } else if IsIconic(self.window) == 0 {
            ShowWindow(self.window, SW_MINIMIZE);
        }
    }
    pub unsafe fn remove(&self) {
        if self.present.replace(false) {
            Shell_NotifyIconW(NIM_DELETE, &self.data());
        }
    }
}
impl Drop for Tray {
    fn drop(&mut self) {
        unsafe {
            self.remove();
        }
    }
}

pub unsafe fn show(window: HWND) {
    ShowWindow(window, SW_RESTORE);
    SetForegroundWindow(window);
}
// No App reference is held while the menu runs its nested Windows message loop.
pub unsafe fn callback(window: HWND, event: u32) {
    match event {
        WM_LBUTTONUP | WM_LBUTTONDBLCLK => {
            PostMessageW(window, SHOW, 0, 0);
        }
        WM_RBUTTONUP | WM_CONTEXTMENU => {
            let menu = CreatePopupMenu();
            if menu.is_null() {
                return;
            }
            AppendMenuW(menu, MF_STRING, OPEN_ITEM, wide("Open goyvoice").as_ptr());
            AppendMenuW(menu, MF_SEPARATOR, 0, null());
            AppendMenuW(
                menu,
                MF_STRING,
                EXIT_ITEM,
                wide("Exit (end calls)").as_ptr(),
            );
            let mut point: POINT = std::mem::zeroed();
            GetCursorPos(&mut point);
            SetForegroundWindow(window);
            let chosen = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                point.x,
                point.y,
                0,
                window,
                null(),
            );
            DestroyMenu(menu);
            PostMessageW(window, WM_NULL, 0, 0);
            if chosen as usize == OPEN_ITEM {
                PostMessageW(window, SHOW, 0, 0);
            }
            if chosen as usize == EXIT_ITEM {
                PostMessageW(window, EXIT, 0, 0);
            }
        }
        _ => {}
    }
}
