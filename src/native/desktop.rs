use super::{Result, winerr};
use orbit::geometry::Rect;
use std::mem::size_of;
use std::sync::atomic::{AtomicUsize, Ordering};
use windows::core::w;
static NEXT_COOKIE: AtomicUsize = AtomicUsize::new(1);
const COOKIE: windows::core::PCWSTR = w!("Seffy.Orbit.3D6905C9.WindowIdentity");
use windows::Win32::{
    Foundation::*,
    Graphics::{Dwm::*, Gdi::*},
    Security::*,
    System::Threading::*,
    UI::{HiDpi::*, WindowsAndMessaging::*},
};
use windows::core::BOOL;

pub fn rect(r: RECT) -> Rect {
    Rect::new(r.left, r.top, r.right - r.left, r.bottom - r.top)
}
#[derive(Clone, Copy)]
pub struct Monitor {
    pub handle: HMONITOR,
    pub work: Rect,
    pub dpi: u32,
}

pub fn monitors() -> Vec<Monitor> {
    unsafe extern "system" fn collect(
        handle: HMONITOR,
        _: HDC,
        _: *mut RECT,
        data: LPARAM,
    ) -> BOOL {
        unsafe {
            let list = &mut *(data.0 as *mut Vec<Monitor>);
            let mut info = MONITORINFO {
                cbSize: size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if GetMonitorInfoW(handle, &mut info).as_bool() {
                let (mut x, mut y) = (96, 96);
                let _ = GetDpiForMonitor(handle, MDT_EFFECTIVE_DPI, &mut x, &mut y);
                list.push(Monitor {
                    handle,
                    work: rect(info.rcWork),
                    dpi: x.max(96),
                });
            }
            TRUE
        }
    }
    let mut list: Vec<Monitor> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM(&mut list as *mut _ as isize),
        );
    }
    list.sort_by_key(|m| (m.work.x, m.work.y));
    list
}

pub fn cursor() -> Result<POINT> {
    let mut p = POINT::default();
    unsafe {
        GetCursorPos(&mut p).map_err(winerr)?;
    }
    Ok(p)
}

pub fn visible_frame(hwnd: HWND) -> Result<Rect> {
    unsafe {
        let mut r = RECT::default();
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut r as *mut _ as _,
            size_of::<RECT>() as u32,
        )
        .is_err()
        {
            GetWindowRect(hwnd, &mut r).map_err(winerr)?;
        }
        Ok(rect(r))
    }
}

#[derive(Clone)]
pub struct Snapshot {
    pub hwnd: HWND,
    pub pid: u32,
    pub placement: WINDOWPLACEMENT,
    pub frame: Rect,
    pub restored: Rect,
    pub monitor: HMONITOR,
    cookie: HANDLE,
}

impl Snapshot {
    pub fn capture() -> Result<Self> {
        unsafe {
            let hwnd = GetAncestor(GetForegroundWindow(), GA_ROOT);
            if hwnd.is_invalid()
                || hwnd == GetDesktopWindow()
                || hwnd == GetShellWindow()
                || !IsWindowVisible(hwnd).as_bool()
            {
                return Err("Select an application window first.".into());
            }
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == GetCurrentProcessId() {
                return Err("Select another application's window, then try again.".into());
            }
            let mut class = [0u16; 128];
            let len = GetClassNameW(hwnd, &mut class);
            if matches!(
                String::from_utf16_lossy(&class[..len.max(0) as usize]).as_str(),
                "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "Progman" | "WorkerW"
            ) {
                return Err("The Windows desktop and taskbar cannot be arranged.".into());
            }
            let mut cloaked = 0u32;
            let _ = DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut cloaked as *mut _ as _, 4);
            if cloaked != 0 {
                return Err("This window is on another desktop.".into());
            }
            if IsHungAppWindow(hwnd).as_bool() {
                return Err(
                    "This application is not responding. Try again when it responds.".into(),
                );
            }
            if elevated(pid).unwrap_or(false) && !elevated(GetCurrentProcessId()).unwrap_or(false) {
                return Err("This application runs as administrator. Use Orbit's tray menu to restart as administrator.".into());
            }
            let mut placement = WINDOWPLACEMENT {
                length: size_of::<WINDOWPLACEMENT>() as u32,
                ..Default::default()
            };
            GetWindowPlacement(hwnd, &mut placement).map_err(winerr)?;
            let frame = visible_frame(hwnd)?;
            let restored = rect(placement.rcNormalPosition);
            // A window property disappears when the HWND is destroyed, so undo
            // cannot target a later window that reuses the same HWND and PID.
            let mut cookie = GetPropW(hwnd, COOKIE);
            if cookie.is_invalid() {
                cookie = HANDLE(NEXT_COOKIE.fetch_add(1, Ordering::Relaxed) as _);
                SetPropW(hwnd, COOKIE, Some(cookie)).map_err(winerr)?;
            }
            Ok(Self {
                hwnd,
                pid,
                placement,
                frame,
                restored,
                monitor: MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST),
                cookie,
            })
        }
    }
    pub fn valid(&self) -> bool {
        unsafe {
            let mut pid = 0;
            GetWindowThreadProcessId(self.hwnd, Some(&mut pid));
            IsWindow(Some(self.hwnd)).as_bool()
                && pid == self.pid
                && GetPropW(self.hwnd, COOKIE) == self.cookie
        }
    }
    pub fn current_is_target(&self) -> bool {
        unsafe { self.valid() && GetAncestor(GetForegroundWindow(), GA_ROOT) == self.hwnd }
    }
    pub fn apply(&self, desired: Rect, maximize: bool) -> Result<()> {
        if !self.valid() {
            return Err("The selected window has closed.".into());
        }
        unsafe {
            let style = GetWindowLongPtrW(self.hwnd, GWL_STYLE) as u32;
            if style & WS_THICKFRAME.0 == 0
                && style & WS_MAXIMIZEBOX.0 == 0
                && ((desired.width - self.frame.width).abs() > 2
                    || (desired.height - self.frame.height).abs() > 2)
            {
                return Err(
                    "This window has a fixed size. Use Center or the Windows move controls.".into(),
                );
            }
            if maximize {
                if GetWindowLongPtrW(self.hwnd, GWL_STYLE) as u32 & WS_MAXIMIZEBOX.0 == 0 {
                    return self.apply(desired, false);
                }
                if !ShowWindowAsync(self.hwnd, SW_MAXIMIZE).as_bool() {
                    return Err("Windows could not maximize this application.".into());
                }
                return Ok(());
            }
            // Restore first: maximized windows have different DWM resize borders.
            // Bound the foreign-window call; input hooks run on their own thread.
            if (IsZoomed(self.hwnd).as_bool() || IsIconic(self.hwnd).as_bool())
                && SendMessageTimeoutW(
                    self.hwnd,
                    WM_SYSCOMMAND,
                    WPARAM(SC_RESTORE as usize),
                    LPARAM(0),
                    SMTO_ABORTIFHUNG | SMTO_BLOCK,
                    200,
                    None,
                )
                .0 == 0
            {
                return Err(
                    "This application could not be restored in time. Try again when it responds."
                        .into(),
                );
            }
            if !self.current_is_target() {
                return Err("The active window changed. Arrangement cancelled.".into());
            }
            let mut outer = RECT::default();
            GetWindowRect(self.hwnd, &mut outer).map_err(winerr)?;
            let visible = visible_frame(self.hwnd)?;
            // DWM's visible bounds omit the invisible resize borders in GetWindowRect.
            let left = visible.x - outer.left;
            let top = visible.y - outer.top;
            let right = outer.right - visible.right();
            let bottom = outer.bottom - visible.bottom();
            SetWindowPos(
                self.hwnd,
                None,
                desired.x - left,
                desired.y - top,
                desired.width + left + right,
                desired.height + top + bottom,
                SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_ASYNCWINDOWPOS,
            )
            .map_err(winerr)
        }
    }
    pub fn undo(&self) -> Result<()> {
        if !self.valid() {
            return Err("That window has closed. There is nothing to restore.".into());
        }
        let mut p = self.placement;
        p.flags |= WPF_ASYNCWINDOWPLACEMENT;
        unsafe { SetWindowPlacement(self.hwnd, &p).map_err(winerr) }
    }
}

pub fn elevated(pid: u32) -> Result<bool> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).map_err(winerr)?;
        let mut token = HANDLE::default();
        let opened = OpenProcessToken(process, TOKEN_QUERY, &mut token);
        let _ = CloseHandle(process);
        opened.map_err(winerr)?;
        let mut value = TOKEN_ELEVATION::default();
        let mut needed = 0;
        let result = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut value as *mut _ as _),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut needed,
        );
        let _ = CloseHandle(token);
        result.map_err(winerr)?;
        Ok(value.TokenIsElevated != 0)
    }
}
