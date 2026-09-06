//! Fluent-inspired native rendering; controls retain Windows keyboard/accessibility behavior.
use std::{
    mem::{size_of, zeroed},
    ptr::null_mut,
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::{Dwm::*, Gdi::*},
    UI::{
        Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW},
        Controls::*,
        Input::KeyboardAndMouse::IsWindowEnabled,
        WindowsAndMessaging::*,
    },
};

const fn rgb(r: u32, g: u32, b: u32) -> u32 {
    r | (g << 8) | (b << 16)
}
pub const BACKGROUND: u32 = rgb(243, 243, 243);
const SURFACE: u32 = rgb(255, 255, 255);
const TEXT: u32 = rgb(36, 36, 36);
const MUTED: u32 = rgb(97, 97, 97);
const ACCENT: u32 = rgb(0, 95, 184);
const BORDER: u32 = rgb(229, 229, 229);
pub struct Motion {
    from: [f32; 3],
    target: [f32; 3],
    started: u64,
}
impl Motion {
    pub fn new(target: [f32; 3]) -> Self {
        Self {
            from: target,
            target,
            started: 0,
        }
    }
    pub fn active(&self, now: u64) -> bool {
        self.from != self.target && now.saturating_sub(self.started) < 180
    }
    pub fn sample(&self, now: u64) -> [f32; 3] {
        let t = (now.saturating_sub(self.started) as f32 / 180.0).min(1.0);
        let eased = 1.0 - (1.0 - t).powi(3);
        std::array::from_fn(|i| self.from[i] + (self.target[i] - self.from[i]) * eased)
    }
    pub fn update(&mut self, target: [f32; 3], now: u64, animate: bool) -> [f32; 3] {
        if target != self.target {
            self.from = self.sample(now);
            self.target = target;
            self.started = now;
        }
        if !animate {
            self.from = target;
        }
        self.sample(now)
    }
}
pub unsafe fn animations_enabled() -> bool {
    let mut enabled: i32 = 0;
    !high_contrast()
        && SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            &mut enabled as *mut _ as _,
            0,
        ) != 0
        && enabled != 0
}
pub unsafe fn control_state(draw: &NMCUSTOMDRAW) -> [f32; 3] {
    [
        (draw.uItemState & CDIS_HOT != 0) as u8 as f32,
        (draw.uItemState & CDIS_SELECTED != 0) as u8 as f32,
        (SendMessageW(draw.hdr.hwndFrom, BM_GETCHECK, 0, 0) == BST_CHECKED as isize) as u8 as f32,
    ]
}
fn blend(a: u32, b: u32, t: f32) -> u32 {
    let channel = |shift: u32| {
        let x = ((a >> shift) & 255) as f32;
        let y = ((b >> shift) & 255) as f32;
        (x + (y - x) * t).round() as u32
    };
    rgb(channel(0), channel(8), channel(16))
}
pub unsafe fn progress(dc: HDC, hwnd: HWND, amount: f32) {
    let s = |v: i32| v * windows_sys::Win32::UI::HiDpi::GetDpiForWindow(hwnd).max(96) as i32 / 96;
    let mut r = RECT {
        left: s(44),
        top: s(134),
        right: s(356),
        bottom: s(137),
    };
    let accent = if high_contrast() {
        GetSysColor(COLOR_HIGHLIGHT)
    } else {
        ACCENT
    };
    fill(
        dc,
        &r,
        if high_contrast() {
            GetSysColor(COLOR_WINDOW)
        } else {
            BORDER
        },
    );
    r.right = r.left + ((r.right - r.left) as f32 * amount.clamp(0.0, 1.0)) as i32;
    fill(dc, &r, accent);
}

pub unsafe fn high_contrast() -> bool {
    let mut hc: HIGHCONTRASTW = zeroed();
    hc.cbSize = size_of::<HIGHCONTRASTW>() as u32;
    SystemParametersInfoW(SPI_GETHIGHCONTRAST, hc.cbSize, &mut hc as *mut _ as _, 0) != 0
        && hc.dwFlags & HCF_HIGHCONTRASTON != 0
}
pub unsafe fn frame(hwnd: HWND) {
    let corner: u32 = DWMWCP_ROUND as u32;
    let _ = DwmSetWindowAttribute(
        hwnd,
        DWMWA_WINDOW_CORNER_PREFERENCE as u32,
        &corner as *const _ as _,
        4,
    );
    let color = if high_contrast() {
        DWMWA_COLOR_DEFAULT
    } else {
        BACKGROUND
    };
    let _ = DwmSetWindowAttribute(hwnd, DWMWA_CAPTION_COLOR as u32, &color as *const _ as _, 4);
}
unsafe fn fill(dc: HDC, r: &RECT, color: u32) {
    let brush = CreateSolidBrush(color);
    FillRect(dc, r, brush);
    DeleteObject(brush);
}
unsafe fn round(dc: HDC, r: &RECT, radius: i32, color: u32, edge: u32) {
    let brush = CreateSolidBrush(color);
    let pen = CreatePen(PS_SOLID, 1, edge);
    let old_b = SelectObject(dc, brush);
    let old_p = SelectObject(dc, pen);
    RoundRect(dc, r.left, r.top, r.right, r.bottom, radius, radius);
    SelectObject(dc, old_b);
    SelectObject(dc, old_p);
    DeleteObject(brush);
    DeleteObject(pen);
}
pub unsafe fn background(dc: HDC, hwnd: HWND) {
    let mut client: RECT = zeroed();
    GetClientRect(hwnd, &mut client);
    if high_contrast() {
        FillRect(dc, &client, GetSysColorBrush(COLOR_WINDOW));
        return;
    }
    fill(dc, &client, BACKGROUND);
    let s = |v: i32| v * windows_sys::Win32::UI::HiDpi::GetDpiForWindow(hwnd).max(96) as i32 / 96;
    round(
        dc,
        &RECT {
            left: s(24),
            top: s(76),
            right: s(376),
            bottom: s(204),
        },
        s(12),
        SURFACE,
        BORDER,
    );
}
pub unsafe fn static_color(dc: HDC, card: bool) -> isize {
    if high_contrast() {
        SetTextColor(dc, GetSysColor(COLOR_WINDOWTEXT));
        SetBkColor(dc, GetSysColor(COLOR_WINDOW));
        return GetSysColorBrush(COLOR_WINDOW) as isize;
    }
    SetTextColor(dc, if card { MUTED } else { TEXT });
    let color = if card { SURFACE } else { BACKGROUND };
    SetBkColor(dc, color);
    SetDCBrushColor(dc, color);
    GetStockObject(DC_BRUSH) as isize
}
pub unsafe fn button(
    draw: &NMCUSTOMDRAW,
    font: HFONT,
    primary: bool,
    toggle: bool,
    motion: [f32; 3],
) -> isize {
    if high_contrast() || draw.dwDrawStage != CDDS_PREPAINT {
        return CDRF_DODEFAULT as isize;
    }
    let dc = draw.hdc;
    let h = draw.hdr.hwndFrom;
    let s = |v: i32| v * windows_sys::Win32::UI::HiDpi::GetDpiForWindow(h).max(96) as i32 / 96;
    let disabled = IsWindowEnabled(h) == 0;
    let [hot, pressed, checked] = motion;
    let mut r = draw.rc;
    let saved = SaveDC(dc);
    SelectObject(dc, font);
    SetBkMode(dc, TRANSPARENT as i32);
    let mut label = vec![0u16; GetWindowTextLengthW(h).max(0) as usize + 1];
    GetWindowTextW(h, label.as_mut_ptr(), label.len() as i32);
    if toggle {
        fill(dc, &r, BACKGROUND);
        round(
            dc,
            &r,
            s(8),
            blend(SURFACE, rgb(250, 250, 250), hot),
            BORDER,
        );
        r.left += s(16);
        SetTextColor(dc, if disabled { MUTED } else { TEXT });
        DrawTextW(
            dc,
            label.as_ptr(),
            -1,
            &mut r,
            DT_LEFT | DT_SINGLELINE | DT_VCENTER,
        );
        let track = RECT {
            left: r.right - s(56),
            top: (r.top + r.bottom - s(20)) / 2,
            right: r.right - s(16),
            bottom: (r.top + r.bottom + s(20)) / 2,
        };
        let color = if disabled {
            rgb(200, 200, 200)
        } else {
            blend(blend(SURFACE, rgb(243, 243, 243), hot), ACCENT, checked)
        };
        round(dc, &track, s(20), color, blend(MUTED, color, checked));
        let left = track.left
            + s(3)
            + ((track.right - track.left - s(20)) as f32 * checked).round() as i32;
        let thumb = RECT {
            left,
            top: track.top + s(3),
            right: left + s(14),
            bottom: track.bottom - s(3),
        };
        let thumb_color = blend(MUTED, SURFACE, checked);
        round(dc, &thumb, s(14), thumb_color, thumb_color);
    } else {
        // The details button sits on the canvas; repair and feedback sit on the card.
        fill(
            dc,
            &r,
            if draw.hdr.idFrom == 104 {
                BACKGROUND
            } else {
                SURFACE
            },
        );
        let color = if disabled {
            rgb(240, 240, 240)
        } else if primary {
            blend(
                blend(ACCENT, rgb(0, 85, 165), hot),
                rgb(0, 70, 140),
                pressed,
            )
        } else {
            blend(
                blend(SURFACE, rgb(249, 249, 249), hot),
                rgb(235, 235, 235),
                pressed,
            )
        };
        round(
            dc,
            &r,
            s(8),
            color,
            if primary || disabled { color } else { BORDER },
        );
        SetTextColor(
            dc,
            if disabled {
                rgb(150, 150, 150)
            } else if primary {
                SURFACE
            } else {
                TEXT
            },
        );
        DrawTextW(
            dc,
            label.as_ptr(),
            -1,
            &mut r,
            DT_CENTER | DT_SINGLELINE | DT_VCENTER,
        );
    }
    if draw.uItemState & CDIS_FOCUS != 0 {
        let mut focus = draw.rc;
        InflateRect(&mut focus, -s(3), -s(3));
        DrawFocusRect(dc, &focus);
    }
    RestoreDC(dc, saved);
    CDRF_SKIPDEFAULT as isize
}

/// Developer preview: ask this application's controls to paint into an offscreen bitmap.
/// This does not capture the desktop or other applications.
pub unsafe fn preview(hwnd: HWND, path: &std::path::Path) -> std::io::Result<()> {
    let mut r: RECT = zeroed();
    GetClientRect(hwnd, &mut r);
    let dc = GetDC(hwnd);
    let mem = CreateCompatibleDC(dc);
    let mut info: BITMAPINFO = zeroed();
    info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth = r.right;
    info.bmiHeader.biHeight = -r.bottom;
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    let mut bits = null_mut();
    let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
    if bitmap.is_null() {
        DeleteDC(mem);
        ReleaseDC(hwnd, dc);
        return Err(std::io::Error::last_os_error());
    }
    let old = SelectObject(mem, bitmap);
    background(mem, hwnd);
    let mut child = GetWindow(hwnd, GW_CHILD);
    while !child.is_null() {
        // Check the child's own visibility: a preview host may intentionally be hidden.
        if GetWindowLongPtrW(child, GWL_STYLE) as u32 & WS_VISIBLE != 0 {
            let saved = SaveDC(mem);
            let mut rect: RECT = zeroed();
            GetWindowRect(child, &mut rect);
            let mut point = POINT {
                x: rect.left,
                y: rect.top,
            };
            ScreenToClient(hwnd, &mut point);
            SetViewportOrgEx(mem, point.x, point.y, null_mut());
            IntersectClipRect(mem, 0, 0, rect.right - rect.left, rect.bottom - rect.top);
            SendMessageW(child, WM_PRINTCLIENT, mem as usize, PRF_CLIENT as isize);
            RestoreDC(mem, saved);
        }
        child = GetWindow(child, GW_HWNDNEXT);
    }
    GdiFlush();
    let pixels = std::slice::from_raw_parts(bits as *const u8, (r.right * r.bottom * 4) as usize);
    let mut data = Vec::with_capacity(54 + pixels.len());
    data.extend_from_slice(b"BM");
    data.extend_from_slice(&((54 + pixels.len()) as u32).to_le_bytes());
    data.extend_from_slice(&[0; 4]);
    data.extend_from_slice(&54u32.to_le_bytes());
    data.extend_from_slice(&40u32.to_le_bytes());
    data.extend_from_slice(&r.right.to_le_bytes());
    data.extend_from_slice(&(-r.bottom).to_le_bytes());
    data.extend_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&32u16.to_le_bytes());
    data.extend_from_slice(&[0; 24]);
    data.extend_from_slice(pixels);
    let result = std::fs::write(path, data);
    SelectObject(mem, old);
    DeleteObject(bitmap);
    DeleteDC(mem);
    ReleaseDC(hwnd, dc);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn motion_finishes_and_respects_disabled_animation() {
        let mut motion = Motion::new([0.0; 3]);
        assert_eq!(motion.update([1.0; 3], 1000, true), [0.0; 3]);
        let half = motion.sample(1090)[0];
        assert!(half > 0.0 && half < 1.0);
        assert_eq!(motion.sample(1180), [1.0; 3]);
        assert!(!motion.active(1180));
        assert_eq!(motion.update([0.0; 3], 1200, false), [0.0; 3]);
        assert!(!motion.active(1200));
    }
}
