#[cfg(target_os = "windows")]
pub mod gdi;
#[cfg(target_os = "windows")]
pub mod win32_gui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeWindowHandle {
    raw: isize,
}

impl NativeWindowHandle {
    pub const fn none() -> Self {
        Self { raw: 0 }
    }

    #[cfg(target_os = "windows")]
    pub fn from_hwnd(hwnd: windows_sys::Win32::Foundation::HWND) -> Self {
        Self { raw: hwnd as isize }
    }

    pub const fn raw(self) -> isize {
        self.raw
    }

    pub const fn describe(self) -> &'static str {
        if self.raw == 0 { "none" } else { "provided" }
    }
}

pub fn default_owner_window() -> NativeWindowHandle {
    #[cfg(target_os = "windows")]
    {
        NativeWindowHandle::from_hwnd(std::ptr::null_mut())
    }

    #[cfg(not(target_os = "windows"))]
    {
        NativeWindowHandle::none()
    }
}
