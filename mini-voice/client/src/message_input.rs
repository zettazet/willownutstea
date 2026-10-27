use windows_sys::Win32::{
    Foundation::*,
    UI::{
        Input::{Ime::*, KeyboardAndMouse::*},
        Shell::*,
        WindowsAndMessaging::*,
    },
};
struct Input {
    parent: HWND,
    composing: bool,
    ime_enter: bool,
}
pub unsafe fn attach(edit: HWND, parent: HWND) -> anyhow::Result<()> {
    let state = Box::into_raw(Box::new(Input {
        parent,
        composing: false,
        ime_enter: false,
    }));
    if SetWindowSubclass(edit, Some(proc), 1, state as usize) == 0 {
        drop(Box::from_raw(state));
        anyhow::bail!("Cannot initialize message input");
    }
    Ok(())
}
unsafe extern "system" fn proc(
    h: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _id: usize,
    data: usize,
) -> LRESULT {
    let state = &mut *(data as *mut Input);
    match msg {
        WM_IME_STARTCOMPOSITION => state.composing = true,
        WM_IME_ENDCOMPOSITION => state.composing = false,
        WM_GETDLGCODE => {
            if lp != 0 && (*(lp as *const MSG)).wParam == VK_RETURN as usize {
                return (DLGC_WANTALLKEYS | DLGC_WANTCHARS) as isize;
            }
        }
        WM_KEYDOWN if wp == VK_RETURN as usize => {
            let context = ImmGetContext(h);
            let composition = state.composing
                || (!context.is_null()
                    && ImmGetCompositionStringW(context, GCS_COMPSTR, std::ptr::null_mut(), 0) > 0);
            if !context.is_null() {
                ImmReleaseContext(h, context);
            }
            if composition {
                state.ime_enter = true;
            } else if !state.ime_enter && GetKeyState(VK_SHIFT as i32) >= 0 {
                if lp & (1 << 30) == 0 {
                    PostMessageW(state.parent, WM_COMMAND, 123, 0);
                }
                return 0;
            }
        }
        WM_CHAR if wp == 13 && !state.composing && GetKeyState(VK_SHIFT as i32) >= 0 => return 0,
        WM_KEYUP if wp == VK_RETURN as usize => state.ime_enter = false,
        WM_NCDESTROY => {
            RemoveWindowSubclass(h, Some(proc), 1);
            drop(Box::from_raw(data as *mut Input));
            return DefSubclassProc(h, msg, wp, lp);
        }
        _ => {}
    }
    DefSubclassProc(h, msg, wp, lp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::Cell,
        ptr::{null, null_mut},
    };
    thread_local! {static SENDS:Cell<usize>=const {Cell::new(0)};}
    unsafe extern "system" fn parent(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        if msg == WM_COMMAND && wp == 123 {
            SENDS.with(|count| count.set(count.get() + 1));
            return 0;
        }
        DefWindowProcW(h, msg, wp, lp)
    }
    unsafe fn pump() {
        let mut msg = std::mem::zeroed();
        while PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    unsafe fn enter(edit: HWND, repeat: bool) {
        PostMessageW(
            edit,
            WM_KEYDOWN,
            VK_RETURN as usize,
            if repeat { 1 << 30 } else { 0 },
        );
        pump();
    }
    #[test]
    fn enter_shift_enter_repeat_and_ime_composition() {
        unsafe {
            let class: Vec<u16> = "MiniVoiceInputTest".encode_utf16().chain(Some(0)).collect();
            let instance = windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(null());
            let window_class = WNDCLASSW {
                lpfnWndProc: Some(parent),
                hInstance: instance,
                lpszClassName: class.as_ptr(),
                ..std::mem::zeroed()
            };
            assert_ne!(RegisterClassW(&window_class), 0);
            let window = CreateWindowExW(
                0,
                class.as_ptr(),
                null(),
                0,
                0,
                0,
                400,
                200,
                null_mut(),
                null_mut(),
                instance,
                null(),
            );
            let edit_class: Vec<u16> = "EDIT".encode_utf16().chain(Some(0)).collect();
            let edit = CreateWindowExW(
                0,
                edit_class.as_ptr(),
                null(),
                WS_CHILD | ES_MULTILINE as u32 | ES_WANTRETURN as u32,
                0,
                0,
                300,
                100,
                window,
                null_mut(),
                instance,
                null(),
            );
            assert!(!edit.is_null());
            attach(edit, window).unwrap();
            enter(edit, false);
            assert_eq!(SENDS.with(Cell::get), 1);
            enter(edit, true);
            assert_eq!(SENDS.with(Cell::get), 1);
            SendMessageW(edit, WM_KEYUP, VK_RETURN as usize, 0);
            let mut keys = [0u8; 256];
            GetKeyboardState(keys.as_mut_ptr());
            let previous = keys;
            keys[VK_SHIFT as usize] = 0x80;
            SetKeyboardState(keys.as_ptr());
            enter(edit, false);
            SetKeyboardState(previous.as_ptr());
            let mut text = [0u16; 32];
            GetWindowTextW(edit, text.as_mut_ptr(), 32);
            assert!(String::from_utf16_lossy(&text).contains('\n'));
            assert_eq!(SENDS.with(Cell::get), 1);
            SendMessageW(edit, WM_IME_STARTCOMPOSITION, 0, 0);
            enter(edit, false);
            SendMessageW(edit, WM_IME_ENDCOMPOSITION, 0, 0);
            enter(edit, true);
            assert_eq!(SENDS.with(Cell::get), 1);
            SendMessageW(edit, WM_KEYUP, VK_RETURN as usize, 0);
            enter(edit, false);
            assert_eq!(SENDS.with(Cell::get), 2);
            DestroyWindow(window);
            UnregisterClassW(class.as_ptr(), instance);
        }
    }
}
