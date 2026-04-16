use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use anyhow::{anyhow, Result};
use winit::raw_window_handle::RawWindowHandle;

pub trait WindowOps: Send + Sync {
    fn show(&self);
    fn hide(&self);
    fn focus(&self);
    fn request_close(&self);
}

#[derive(Clone)]
pub struct WindowController {
    hwnd: isize,
    allow_native_close: Arc<AtomicBool>,
}

impl WindowController {
    pub fn from_raw_window_handle(raw_handle: RawWindowHandle) -> Result<Self> {
        match raw_handle {
            RawWindowHandle::Win32(handle) => Ok(Self {
                hwnd: handle.hwnd.get(),
                allow_native_close: Arc::new(AtomicBool::new(false)),
            }),
            _ => Err(anyhow!("unsupported raw window handle")),
        }
    }

    pub fn show(&self) {
        <Self as WindowOps>::show(self);
    }

    pub fn hide_to_tray(&self) {
        <Self as WindowOps>::hide(self);
    }

    pub fn focus(&self) {
        <Self as WindowOps>::focus(self);
    }

    pub fn request_exit(&self) {
        <Self as WindowOps>::request_close(self);
    }

    pub fn allow_native_close(&self) -> bool {
        self.allow_native_close.load(Ordering::SeqCst)
    }

    pub fn clear_native_close_permission(&self) {
        self.allow_native_close.store(false, Ordering::SeqCst);
    }

    #[cfg(test)]
    pub fn hwnd_for_test(&self) -> isize {
        self.hwnd
    }
}

impl WindowOps for WindowController {
    fn show(&self) {
        #[cfg(target_os = "windows")]
        unsafe {
            let hwnd = self.hwnd as *mut std::ffi::c_void;
            use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_RESTORE, SW_SHOW};

            ShowWindow(hwnd, SW_SHOW);
            ShowWindow(hwnd, SW_RESTORE);
        }
    }

    fn hide(&self) {
        #[cfg(target_os = "windows")]
        unsafe {
            let hwnd = self.hwnd as *mut std::ffi::c_void;
            use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};

            ShowWindow(hwnd, SW_HIDE);
        }
    }

    fn focus(&self) {
        #[cfg(target_os = "windows")]
        unsafe {
            let hwnd = self.hwnd as *mut std::ffi::c_void;
            use windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow;

            SetForegroundWindow(hwnd);
        }
    }

    fn request_close(&self) {
        self.allow_native_close.store(true, Ordering::SeqCst);
        #[cfg(target_os = "windows")]
        unsafe {
            let hwnd = self.hwnd as *mut std::ffi::c_void;
            use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

            PostMessageW(hwnd, WM_CLOSE, 0, 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use core::num::NonZeroIsize;

    use winit::raw_window_handle::{RawWindowHandle, Win32WindowHandle};

    #[test]
    fn controller_extracts_hwnd_from_win32_raw_handle() {
        let handle = Win32WindowHandle::new(NonZeroIsize::new(7).expect("non zero"));
        let controller =
            super::WindowController::from_raw_window_handle(RawWindowHandle::Win32(handle))
                .expect("controller");

        assert_eq!(controller.hwnd_for_test(), 7);
    }
}
