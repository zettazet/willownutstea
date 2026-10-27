use mini_client::avatars::{Cache, ICON_SIZE};
use mini_shared::{Call, MAX_AVATAR_UPLOAD};
use std::{io::Read, os::windows::ffi::OsStringExt};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    UI::{Controls::Dialogs::*, WindowsAndMessaging::*},
};
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
pub const PICK: u32 = WM_APP + 10;
pub unsafe fn choose(window: HWND) -> anyhow::Result<Option<Vec<u8>>> {
    let mut name = [0u16; 4096];
    let filter = wide("Images (PNG, JPEG, WebP)\0*.png;*.jpg;*.jpeg;*.webp\0\0");
    let mut dialog: OPENFILENAMEW = std::mem::zeroed();
    dialog.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    dialog.hwndOwner = window;
    dialog.lpstrFile = name.as_mut_ptr();
    dialog.nMaxFile = name.len() as u32;
    dialog.lpstrFilter = filter.as_ptr();
    dialog.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR;
    if GetOpenFileNameW(&mut dialog) == 0 {
        anyhow::ensure!(CommDlgExtendedError() == 0, "Cannot open file picker");
        return Ok(None);
    }
    let len = name.iter().position(|c| *c == 0).unwrap_or(name.len());
    let file = std::fs::File::open(std::ffi::OsString::from_wide(&name[..len]))?;
    anyhow::ensure!(
        file.metadata()?.len() <= MAX_AVATAR_UPLOAD as u64,
        "Avatar must be at most 5 MiB"
    );
    let mut bytes = Vec::new();
    file.take(MAX_AVATAR_UPLOAD as u64 + 1)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_AVATAR_UPLOAD,
        "Avatar must be 1 byte–5 MiB"
    );
    Ok(Some(bytes))
}
/// Paint cached square 40px thumbnails directly; no persistent GDI bitmap per avatar.
pub unsafe fn draw(hdc: HDC, rect: RECT, call: Option<&Call>, enabled: bool, cache: &Cache) {
    SetDCBrushColor(hdc, 0x00202020);
    FillRect(hdc, &rect, GetStockObject(DC_BRUSH) as HBRUSH);
    SetTextColor(hdc, 0x00dedede);
    SetBkMode(hdc, TRANSPARENT as i32);
    let Some(call) = call else {
        return;
    };
    if !enabled {
        let text = call
            .participants
            .iter()
            .map(|p| {
                format!(
                    "{}{}{}   ",
                    p.user.username,
                    if p.labeled { " — gay" } else { "" },
                    if p.mic_muted { " [muted]" } else { "" }
                )
            })
            .collect::<String>();
        let mut area = rect;
        DrawTextW(hdc, wide(&text).as_ptr(), -1, &mut area, DT_WORDBREAK);
        return;
    }
    let width = (rect.right - rect.left) / 4;
    for (i, p) in call.participants.iter().enumerate() {
        let x = rect.left + (i % 4) as i32 * width;
        let size = ICON_SIZE as i32;
        let y = rect.top + (i / 4) as i32 * (size + 4);
        if let Some(image) = cache.get(p.user.id) {
            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            info.bmiHeader.biWidth = size;
            info.bmiHeader.biHeight = -size;
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            info.bmiHeader.biCompression = BI_RGB;
            StretchDIBits(
                hdc,
                x,
                y,
                size,
                size,
                0,
                0,
                size,
                size,
                image.pixels.as_ptr() as _,
                &info,
                DIB_RGB_COLORS,
                SRCCOPY,
            );
        } else {
            SetDCBrushColor(hdc, 0x00785488);
            let old = SelectObject(hdc, GetStockObject(DC_BRUSH));
            Rectangle(hdc, x, y, x + size, y + size);
            SelectObject(hdc, old);
            let mut r = RECT {
                left: x,
                top: y + 11,
                right: x + size,
                bottom: y + size,
            };
            DrawTextW(
                hdc,
                wide(&p.user.username.chars().next().unwrap_or('?').to_string()).as_ptr(),
                -1,
                &mut r,
                DT_CENTER | DT_SINGLELINE,
            );
        }
        let mut name_area = RECT {
            left: x + size + 4,
            top: y,
            right: x + width - 2,
            bottom: y + 16,
        };
        DrawTextW(
            hdc,
            wide(&p.user.username).as_ptr(),
            -1,
            &mut name_area,
            DT_SINGLELINE | DT_END_ELLIPSIS,
        );
        let flags = format!(
            "{}{}",
            if p.labeled { "gay" } else { "" },
            if p.mic_muted { " [muted]" } else { "" }
        );
        let mut flags_area = RECT {
            left: x + size + 4,
            top: y + 20,
            right: x + width - 2,
            bottom: y + size,
        };
        DrawTextW(
            hdc,
            wide(&flags).as_ptr(),
            -1,
            &mut flags_area,
            DT_SINGLELINE | DT_END_ELLIPSIS,
        );
    }
}

#[derive(Default)]
pub struct View {
    pub call: Option<Call>,
    pub enabled: bool,
    pub cache: Cache,
}
pub fn property() -> Vec<u16> {
    wide("MiniVoiceProfileView")
}
