//! Accès Win32 minimal, déclaré directement (user32/kernel32) : pas de crate `windows`, donc aucun conflit
//! de version avec Tauri. Sur les autres OS : implémentations neutres pour le développement.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameWindow {
    pub hwnd: isize,
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[cfg(windows)]
mod imp {
    use super::GameWindow;

    #[repr(C)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }
    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn GetForegroundWindow() -> isize;
        fn FindWindowW(class: *const u16, title: *const u16) -> isize;
        fn GetClientRect(hwnd: isize, rect: *mut Rect) -> i32;
        fn ClientToScreen(hwnd: isize, pt: *mut Point) -> i32;
        fn GetWindowLongPtrW(hwnd: isize, index: i32) -> isize;
        fn SetWindowLongPtrW(hwnd: isize, index: i32, value: isize) -> isize;
        fn GetClipboardSequenceNumber() -> u32;
        fn IsIconic(hwnd: isize) -> i32;
    }

    const GWL_EXSTYLE: i32 = -20;
    const WS_EX_TRANSPARENT: isize = 0x0000_0020;
    const WS_EX_TOOLWINDOW: isize = 0x0000_0080;
    const WS_EX_LAYERED: isize = 0x0008_0000;
    const WS_EX_NOACTIVATE: isize = 0x0800_0000;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn foreground() -> isize {
        unsafe { GetForegroundWindow() }
    }

    pub fn find_game(title: &str) -> Option<GameWindow> {
        unsafe {
            let class = wide("POEWindowClass");
            let mut hwnd = FindWindowW(class.as_ptr(), std::ptr::null());
            if hwnd == 0 {
                let t = wide(title);
                hwnd = FindWindowW(std::ptr::null(), t.as_ptr());
            }
            if hwnd == 0 || IsIconic(hwnd) != 0 {
                return None;
            }
            let mut r = Rect { left: 0, top: 0, right: 0, bottom: 0 };
            if GetClientRect(hwnd, &mut r) == 0 {
                return None;
            }
            let mut p = Point { x: 0, y: 0 };
            if ClientToScreen(hwnd, &mut p) == 0 {
                return None;
            }
            Some(GameWindow { hwnd, left: p.x, top: p.y, right: p.x + (r.right - r.left), bottom: p.y + (r.bottom - r.top) })
        }
    }

    /// Fenêtre d'overlay : ne prend jamais le focus (NOACTIVATE), absente d'Alt+Tab (TOOLWINDOW).
    /// Le clic-traversant (TRANSPARENT) est géré par Tauri via `set_ignore_cursor_events`.
    pub fn apply_overlay_styles(hwnd: isize) {
        unsafe {
            let cur = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, cur | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_LAYERED);
            let _ = WS_EX_TRANSPARENT;
        }
    }

    pub fn clipboard_seq() -> u32 {
        unsafe { GetClipboardSequenceNumber() }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::GameWindow;
    pub fn foreground() -> isize {
        0
    }
    pub fn find_game(_title: &str) -> Option<GameWindow> {
        None
    }
    pub fn apply_overlay_styles(_hwnd: isize) {}
    pub fn clipboard_seq() -> u32 {
        0
    }
}

pub use imp::*;
