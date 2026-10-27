//! Small GDI palette; no renderer, bitmap backgrounds or animation loop.
use windows_sys::Win32::{
    Graphics::Gdi::*,
    UI::{Controls::*, WindowsAndMessaging::*},
};
pub const BACKGROUND: u32 = 0x00263038; // cocoa (COLORREF is BGR)
pub const SURFACE: u32 = 0x00333f49;
pub const TEXT: u32 = 0x00dfedf6;
pub const ACCENT: u32 = 0x008eb4df;
pub const INK: u32 = 0x00212c37;
pub struct Theme {
    pub font: HFONT,
    pub bold_font: HFONT,
}
impl Theme {
    pub unsafe fn new() -> Self {
        let face: Vec<u16> = "Segoe UI".encode_utf16().chain(Some(0)).collect();
        Self {
            bold_font: CreateFontW(
                -15,
                0,
                0,
                0,
                700,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                OUT_DEFAULT_PRECIS as u32,
                CLIP_DEFAULT_PRECIS as u32,
                CLEARTYPE_QUALITY as u32,
                DEFAULT_PITCH as u32,
                face.as_ptr(),
            ),
            font: CreateFontW(
                -15,
                0,
                0,
                0,
                400,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                OUT_DEFAULT_PRECIS as u32,
                CLIP_DEFAULT_PRECIS as u32,
                CLEARTYPE_QUALITY as u32,
                DEFAULT_PITCH as u32,
                face.as_ptr(),
            ),
        }
    }
}
impl Drop for Theme {
    fn drop(&mut self) {
        unsafe {
            if !self.bold_font.is_null() {
                DeleteObject(self.bold_font);
            }
            if !self.font.is_null() {
                DeleteObject(self.font);
            }
        }
    }
}
pub unsafe fn button(draw: &DRAWITEMSTRUCT) {
    let dc = draw.hDC;
    let saved = SaveDC(dc);
    let font = SendMessageW(draw.hwndItem, WM_GETFONT, 0, 0);
    if font != 0 {
        SelectObject(dc, font as HGDIOBJ);
    }
    let mut r = draw.rcItem;
    SetDCBrushColor(dc, BACKGROUND);
    FillRect(dc, &r, GetStockObject(DC_BRUSH) as HBRUSH);
    let pressed = draw.itemState & ODS_SELECTED != 0;
    let disabled = draw.itemState & ODS_DISABLED != 0;
    let accent = matches!(draw.CtlID, 104 | 105 | 123 | 124 | 125);
    let color = if disabled {
        SURFACE
    } else if pressed {
        0x006b8daf
    } else if accent {
        ACCENT
    } else {
        0x00596576
    };
    SetDCBrushColor(dc, color);
    SelectObject(dc, GetStockObject(DC_BRUSH));
    SelectObject(dc, GetStockObject(NULL_PEN));
    RoundRect(dc, r.left, r.top, r.right, r.bottom, 10, 10);
    SetBkMode(dc, TRANSPARENT as i32);
    SetTextColor(dc, if accent && !disabled { INK } else { TEXT });
    let mut label = [0u16; 96];
    GetWindowTextW(draw.hwndItem, label.as_mut_ptr(), label.len() as i32);
    if pressed {
        OffsetRect(&mut r, 1, 1);
    }
    DrawTextW(
        dc,
        label.as_ptr(),
        -1,
        &mut r,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
    );
    if draw.itemState & ODS_FOCUS != 0 {
        InflateRect(&mut r, -3, -3);
        DrawFocusRect(dc, &r);
    }
    RestoreDC(dc, saved);
}
