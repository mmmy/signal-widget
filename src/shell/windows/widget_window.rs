use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::collections::HashMap;

use crate::adapters::floating_widget::hit_test::{classify_hit, WidgetHitZone};

#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::{POINT, RECT};
#[cfg(target_os = "windows")]
use windows_sys::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMNCRP_DISABLED, DWMWA_NCRENDERING_POLICY,
};
#[cfg(target_os = "windows")]
use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, DefWindowProcW, FindWindowW, GetClientRect, GetCursorPos, GetWindowLongPtrW,
    SetWindowLongPtrW, GWLP_WNDPROC, GWL_EXSTYLE, HTCLIENT, HTTRANSPARENT, WM_NCDESTROY,
    WM_NCHITTEST, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WidgetNativeState {
    pub original_wndproc: isize,
    pub radius: f32,
}

static WIDGET_STATES: Lazy<Mutex<HashMap<isize, WidgetNativeState>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub fn ensure_layered_style(existing: isize) -> isize {
    existing | WS_EX_LAYERED as isize | WS_EX_TOOLWINDOW as isize | WS_EX_NOACTIVATE as isize
}

pub fn register_widget_state(hwnd: isize, original_wndproc: isize, radius: f32) {
    WIDGET_STATES.lock().insert(
        hwnd,
        WidgetNativeState {
            original_wndproc,
            radius,
        },
    );
}

pub fn widget_state(hwnd: isize) -> Option<WidgetNativeState> {
    WIDGET_STATES.lock().get(&hwnd).copied()
}

pub fn clear_widget_state(hwnd: isize) {
    WIDGET_STATES.lock().remove(&hwnd);
}

pub fn hit_zone_to_lresult(zone: WidgetHitZone) -> isize {
    match zone {
        WidgetHitZone::Transparent => HTTRANSPARENT as isize,
        WidgetHitZone::Drag => HTCLIENT as isize,
    }
}

#[cfg(target_os = "windows")]
fn title_to_utf16(title: &str) -> Vec<u16> {
    title.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(target_os = "windows")]
pub unsafe fn find_widget_hwnd(title: &str) -> Option<isize> {
    let wide = title_to_utf16(title);
    let hwnd = FindWindowW(std::ptr::null(), wide.as_ptr());
    if hwnd.is_null() {
        None
    } else {
        Some(hwnd as isize)
    }
}

#[cfg(target_os = "windows")]
pub unsafe fn apply_widget_surface_style(hwnd: isize) {
    let existing = GetWindowLongPtrW(hwnd as _, GWL_EXSTYLE);
    let updated = ensure_layered_style(existing);
    if updated != existing {
        SetWindowLongPtrW(hwnd as _, GWL_EXSTYLE, updated);
    }

    let policy = DWMNCRP_DISABLED;
    let _ = DwmSetWindowAttribute(
        hwnd as _,
        DWMWA_NCRENDERING_POLICY as u32,
        &policy as *const _ as _,
        std::mem::size_of_val(&policy) as u32,
    );
}

#[cfg(target_os = "windows")]
pub unsafe fn install_widget_hit_test(hwnd: isize, radius: f32) {
    if widget_state(hwnd).is_some() {
        return;
    }

    let original = GetWindowLongPtrW(hwnd as _, GWLP_WNDPROC);
    register_widget_state(hwnd, original, radius);
    SetWindowLongPtrW(
        hwnd as _,
        GWLP_WNDPROC,
        widget_wndproc as *const () as isize,
    );
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn widget_wndproc(
    hwnd: *mut std::ffi::c_void,
    msg: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    let hwnd_isize = hwnd as isize;

    if msg == WM_NCHITTEST {
        if let Some(state) = widget_state(hwnd_isize) {
            let mut rect = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            let _ = GetClientRect(hwnd, &mut rect);
            let center = (
                ((rect.right - rect.left) as f32) / 2.0,
                ((rect.bottom - rect.top) as f32) / 2.0,
            );
            let mut point = POINT { x: 0, y: 0 };
            let _ = GetCursorPos(&mut point);
            let _ = ScreenToClient(hwnd, &mut point);
            let zone = classify_hit((point.x as f32, point.y as f32), center, state.radius);
            return hit_zone_to_lresult(zone);
        }
    }

    if msg == WM_NCDESTROY {
        let original = widget_state(hwnd_isize).map(|state| state.original_wndproc);
        clear_widget_state(hwnd_isize);
        if let Some(original) = original {
            SetWindowLongPtrW(hwnd as _, GWLP_WNDPROC, original);
            let original_wndproc: unsafe extern "system" fn(
                *mut std::ffi::c_void,
                u32,
                usize,
                isize,
            ) -> isize = std::mem::transmute(original);
            return CallWindowProcW(Some(original_wndproc), hwnd, msg, wparam, lparam);
        }
    }

    if let Some(state) = widget_state(hwnd_isize) {
        let original_wndproc: unsafe extern "system" fn(
            *mut std::ffi::c_void,
            u32,
            usize,
            isize,
        ) -> isize = std::mem::transmute(state.original_wndproc);
        return CallWindowProcW(Some(original_wndproc), hwnd, msg, wparam, lparam);
    }

    DefWindowProcW(hwnd, msg, wparam, lparam)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_layered_style_sets_required_bit() {
        let style = ensure_layered_style(0);
        assert_ne!(
            style & windows_sys::Win32::UI::WindowsAndMessaging::WS_EX_LAYERED as isize,
            0
        );
    }

    #[test]
    fn ensure_layered_style_preserves_existing_bits() {
        let original = 0x20_isize;
        let updated = ensure_layered_style(original);
        assert_ne!(updated & 0x20, 0);
    }

    #[test]
    fn ensure_layered_style_adds_toolwindow_and_noactivate() {
        let updated = ensure_layered_style(0);
        assert_ne!(updated & WS_EX_TOOLWINDOW as isize, 0);
        assert_ne!(updated & WS_EX_NOACTIVATE as isize, 0);
    }

    #[test]
    fn registry_round_trip_stores_original_proc_and_radius() {
        let hwnd = 7_isize;
        register_widget_state(hwnd, 99, 28.0);
        let state = widget_state(hwnd).expect("registered state");
        assert_eq!(state.original_wndproc, 99);
        assert_eq!(state.radius, 28.0);
        clear_widget_state(hwnd);
    }

    #[test]
    fn widget_hit_zone_maps_to_win32_codes() {
        assert_eq!(
            hit_zone_to_lresult(WidgetHitZone::Transparent),
            windows_sys::Win32::UI::WindowsAndMessaging::HTTRANSPARENT as isize
        );
        assert_eq!(
            hit_zone_to_lresult(WidgetHitZone::Drag),
            windows_sys::Win32::UI::WindowsAndMessaging::HTCLIENT as isize
        );
    }
}
