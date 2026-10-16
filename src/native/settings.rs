use super::{MSG_SETTINGS, Result, wide, winerr};
use orbit::config::Config;
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{Dwm::*, Gdi::*},
        System::LibraryLoader::*,
        UI::{HiDpi::*, WindowsAndMessaging::*},
    },
    core::*,
};

pub const SAVE: usize = 101;
pub const DIAGNOSE: usize = 102;
pub const INSTALL: usize = 103;
pub const FOLDER: usize = 104;

struct Control {
    hwnd: HWND,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    heading: bool,
}
pub struct Settings {
    pub hwnd: HWND,
    controls: Vec<Control>,
    fonts: Vec<HFONT>,
    trigger: HWND,
    delay: HWND,
    gap: HWND,
    preview: HWND,
    startup: HWND,
    status: HWND,
}

impl Settings {
    pub fn new(host: HWND, config: &Config, startup: bool) -> Result<Self> {
        unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
            unsafe {
                match msg {
                    WM_NCCREATE => {
                        let cs = &*(l.0 as *const CREATESTRUCTW);
                        SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
                    }
                    WM_CLOSE => {
                        let _ = ShowWindow(hwnd, SW_HIDE);
                        return LRESULT(0);
                    }
                    WM_COMMAND => {
                        let host = HWND(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as _);
                        let _ =
                            PostMessageW(Some(host), MSG_SETTINGS, WPARAM(w.0 & 0xffff), LPARAM(0));
                        return LRESULT(0);
                    }
                    WM_DPICHANGED => {
                        let r = &*(l.0 as *const RECT);
                        let _ = SetWindowPos(
                            hwnd,
                            None,
                            r.left,
                            r.top,
                            r.right - r.left,
                            r.bottom - r.top,
                            SWP_NOZORDER | SWP_NOACTIVATE,
                        );
                    }
                    WM_SIZE => {
                        let host = HWND(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as _);
                        let _ = PostMessageW(Some(host), MSG_SETTINGS, WPARAM(999), LPARAM(0));
                    }
                    _ => {}
                }
                DefWindowProcW(hwnd, msg, w, l)
            }
        }
        unsafe {
            let instance = GetModuleHandleW(None).map_err(winerr)?;
            let icon = LoadIconW(Some(instance.into()), PCWSTR(1usize as _)).map_err(winerr)?;
            let wc = WNDCLASSW {
                lpfnWndProc: Some(proc),
                hInstance: instance.into(),
                lpszClassName: w!("OrbitSettings"),
                hIcon: icon,
                hCursor: LoadCursorW(None, IDC_ARROW).map_err(winerr)?,
                hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as _),
                ..Default::default()
            };
            if RegisterClassW(&wc) == 0 {
                return Err(winerr(Error::from_thread()));
            }
            let hwnd = CreateWindowExW(
                WS_EX_CONTROLPARENT,
                w!("OrbitSettings"),
                w!("Orbit — Settings"),
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                650,
                610,
                None,
                None,
                Some(instance.into()),
                Some(host.0),
            )
            .map_err(winerr)?;
            let corner = DWMWCP_ROUND;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &corner as *const _ as _,
                size_of_val(&corner) as u32,
            );
            let mut s = Self {
                hwnd,
                controls: Vec::new(),
                fonts: Vec::new(),
                trigger: HWND::default(),
                delay: HWND::default(),
                gap: HWND::default(),
                preview: HWND::default(),
                startup: HWND::default(),
                status: HWND::default(),
            };
            s.add("STATIC", "Orbit", WS_VISIBLE, 0, [28, 22, 540, 42], true)?;
            s.add(
                "STATIC",
                "Your windows. One gesture.",
                WS_VISIBLE,
                0,
                [30, 68, 540, 25],
                false,
            )?;
            s.add("STATIC","Hold Right Ctrl, move toward a layout, then release.\nUse arrows to select, the wheel to change width, and Tab to switch displays.",WS_VISIBLE,0,[30,108,540,56],false)?;
            s.add(
                "STATIC",
                "&Trigger key",
                WS_VISIBLE,
                0,
                [30, 190, 180, 24],
                false,
            )?;
            s.trigger = s.add(
                "COMBOBOX",
                "",
                WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(CBS_DROPDOWNLIST as u32 | WS_VSCROLL.0),
                201,
                [224, 185, 340, 180],
                false,
            )?;
            s.items(s.trigger, &["Right Ctrl (recommended)", "F8", "F9"]);
            s.add(
                "STATIC",
                "&Hold delay",
                WS_VISIBLE,
                0,
                [30, 237, 180, 24],
                false,
            )?;
            s.delay = s.add(
                "COMBOBOX",
                "",
                WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(CBS_DROPDOWNLIST as u32),
                202,
                [224, 232, 340, 180],
                false,
            )?;
            s.items(
                s.delay,
                &["Quick — 80 ms", "Balanced — 180 ms", "Deliberate — 300 ms"],
            );
            s.add(
                "STATIC",
                "Window &gap",
                WS_VISIBLE,
                0,
                [30, 284, 180, 24],
                false,
            )?;
            s.gap = s.add(
                "COMBOBOX",
                "",
                WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(CBS_DROPDOWNLIST as u32),
                203,
                [224, 279, 340, 180],
                false,
            )?;
            s.items(s.gap, &["None", "8 points", "16 points", "24 points"]);
            s.preview = s.add(
                "BUTTON",
                "Show layout &preview",
                WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
                204,
                [30, 330, 534, 26],
                false,
            )?;
            s.startup = s.add(
                "BUTTON",
                "Start Orbit when I sign &in",
                WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
                205,
                [30, 366, 534, 26],
                false,
            )?;
            s.add("STATIC","Win + Alt + Space opens the menu without holding a key.\nWin + Alt + Z restores the last arranged window.",WS_VISIBLE,0,[30,412,534,48],false)?;
            s.status = s.add("STATIC", "", WS_VISIBLE, 0, [30, 470, 534, 40], false)?;
            s.add(
                "BUTTON",
                "&Save",
                WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_DEFPUSHBUTTON as u32),
                SAVE,
                [30, 526, 100, 34],
                false,
            )?;
            s.add(
                "BUTTON",
                "Diagnostics",
                WS_VISIBLE | WS_TABSTOP,
                DIAGNOSE,
                [140, 526, 126, 34],
                false,
            )?;
            s.add(
                "BUTTON",
                "Install",
                WS_VISIBLE | WS_TABSTOP,
                INSTALL,
                [276, 526, 90, 34],
                false,
            )?;
            s.add(
                "BUTTON",
                "Open folder",
                WS_VISIBLE | WS_TABSTOP,
                FOLDER,
                [376, 526, 130, 34],
                false,
            )?;
            s.load(config, startup);
            s.layout();
            Ok(s)
        }
    }
    fn add(
        &mut self,
        class: &str,
        text: &str,
        style: WINDOW_STYLE,
        id: usize,
        box_: [i32; 4],
        heading: bool,
    ) -> Result<HWND> {
        unsafe {
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                PCWSTR(wide(class).as_ptr()),
                PCWSTR(wide(text).as_ptr()),
                WS_CHILD | style,
                0,
                0,
                1,
                1,
                Some(self.hwnd),
                Some(HMENU(id as _)),
                Some(GetModuleHandleW(None).map_err(winerr)?.into()),
                None,
            )
            .map_err(winerr)?;
            self.controls.push(Control {
                hwnd,
                x: box_[0],
                y: box_[1],
                w: box_[2],
                h: box_[3],
                heading,
            });
            Ok(hwnd)
        }
    }
    unsafe fn items(&self, hwnd: HWND, items: &[&str]) {
        unsafe {
            for text in items {
                send(
                    hwnd,
                    CB_ADDSTRING,
                    WPARAM(0),
                    LPARAM(wide(text).as_ptr() as _),
                );
            }
        }
    }
    pub fn load(&self, c: &Config, startup: bool) {
        unsafe {
            send(
                self.trigger,
                CB_SETCURSEL,
                WPARAM(match c.trigger {
                    0x77 => 1,
                    0x78 => 2,
                    _ => 0,
                }),
                LPARAM(0),
            );
            send(
                self.delay,
                CB_SETCURSEL,
                WPARAM(if c.delay_ms <= 80 {
                    0
                } else if c.delay_ms >= 300 {
                    2
                } else {
                    1
                }),
                LPARAM(0),
            );
            send(
                self.gap,
                CB_SETCURSEL,
                WPARAM(match c.gap {
                    0 => 0,
                    1..=8 => 1,
                    9..=16 => 2,
                    _ => 3,
                }),
                LPARAM(0),
            );
            send(
                self.preview,
                BM_SETCHECK,
                WPARAM(c.preview as usize),
                LPARAM(0),
            );
            send(
                self.startup,
                BM_SETCHECK,
                WPARAM(startup as usize),
                LPARAM(0),
            );
        }
    }
    pub fn read(&self) -> (Config, bool) {
        unsafe {
            let selected = |h| send(h, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0.max(0) as usize;
            (
                Config {
                    trigger: [0xA3, 0x77, 0x78][selected(self.trigger).min(2)],
                    delay_ms: [80, 180, 300][selected(self.delay).min(2)],
                    gap: [0, 8, 16, 24][selected(self.gap).min(3)],
                    preview: send(self.preview, BM_GETCHECK, WPARAM(0), LPARAM(0)).0 != 0,
                    first_run: false,
                },
                send(self.startup, BM_GETCHECK, WPARAM(0), LPARAM(0)).0 != 0,
            )
        }
    }
    pub fn status(&self, text: &str) {
        unsafe {
            let _ = SetWindowTextW(self.status, PCWSTR(wide(text).as_ptr()));
        }
    }
    pub fn show(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOWNORMAL);
            let _ = SetForegroundWindow(self.hwnd);
        }
    }
    pub fn layout(&mut self) {
        unsafe {
            let scale = GetDpiForWindow(self.hwnd).max(96) as f64 / 96.;
            let mut bounds = RECT::default();
            let _ = GetWindowRect(self.hwnd, &mut bounds);
            let mut desired = RECT {
                left: 0,
                top: 0,
                right: (594. * scale) as i32,
                bottom: (590. * scale) as i32,
            };
            let _ = AdjustWindowRectExForDpi(
                &mut desired,
                WINDOW_STYLE(GetWindowLongPtrW(self.hwnd, GWL_STYLE) as u32),
                false,
                WS_EX_CONTROLPARENT,
                (96. * scale) as u32,
            );
            if bounds.right - bounds.left != desired.right - desired.left
                || bounds.bottom - bounds.top != desired.bottom - desired.top
            {
                let _ = SetWindowPos(
                    self.hwnd,
                    None,
                    0,
                    0,
                    desired.right - desired.left,
                    desired.bottom - desired.top,
                    SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            let body = CreateFontW(
                -(14. * scale) as i32,
                0,
                0,
                0,
                400,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                DEFAULT_PITCH.0 as u32,
                w!("Segoe UI"),
            );
            let heading = CreateFontW(
                -(31. * scale) as i32,
                0,
                0,
                0,
                600,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                DEFAULT_PITCH.0 as u32,
                w!("Segoe UI"),
            );
            for c in &self.controls {
                let _ = SetWindowPos(
                    c.hwnd,
                    None,
                    (c.x as f64 * scale) as i32,
                    (c.y as f64 * scale) as i32,
                    (c.w as f64 * scale) as i32,
                    (c.h as f64 * scale) as i32,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                send(
                    c.hwnd,
                    WM_SETFONT,
                    WPARAM(if c.heading {
                        heading.0 as usize
                    } else {
                        body.0 as usize
                    }),
                    LPARAM(1),
                );
            }
            for f in self.fonts.drain(..) {
                let _ = DeleteObject(f.into());
            }
            self.fonts = vec![body, heading];
        }
    }
}
impl Drop for Settings {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
            for f in self.fonts.drain(..) {
                let _ = DeleteObject(f.into());
            }
        }
    }
}

unsafe fn send(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe { windows::Win32::UI::WindowsAndMessaging::SendMessageW(hwnd, msg, Some(w), Some(l)) }
}
