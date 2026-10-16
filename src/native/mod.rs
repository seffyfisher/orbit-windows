mod desktop;
mod overlay;
mod settings;
mod shell;

use orbit::{
    config::Config,
    geometry::{self, Layout, Rect},
};
use std::{
    cell::RefCell,
    collections::VecDeque,
    mem::size_of,
    sync::atomic::{AtomicBool, AtomicIsize, AtomicU8, AtomicU32, AtomicU64, Ordering::Relaxed},
    time::Instant,
};
use windows::{
    Win32::{
        Foundation::*,
        System::{Com::*, LibraryLoader::*, RemoteDesktop::*, Threading::*},
        UI::{Controls::*, HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::*,
};

pub type Result<T> = std::result::Result<T, String>;
pub fn winerr(error: Error) -> String {
    error.to_string()
}
pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
const MSG_BEGIN: u32 = WM_APP + 1;
const MSG_END: u32 = WM_APP + 2;
const MSG_POINTER: u32 = WM_APP + 3;
const MSG_KEY: u32 = WM_APP + 4;
const MSG_CANCEL: u32 = WM_APP + 5;
const MSG_WHEEL: u32 = WM_APP + 6;
pub const MSG_SETTINGS: u32 = WM_APP + 10;
pub const MSG_TRAY: u32 = WM_APP + 50;
const MSG_SHOW_SETTINGS: u32 = WM_APP + 51;
const WATCHDOG: usize = 1;
const VERIFY: usize = 2;
const IDLE: u8 = 0;
const PENDING: u8 = 1;
const VISIBLE: u8 = 2;
const CANCELLED: u8 = 3;
const STICKY: u8 = 4;
static HOST: AtomicIsize = AtomicIsize::new(0);
static STATE: AtomicU8 = AtomicU8::new(IDLE);
static HELD: AtomicBool = AtomicBool::new(false);
static RESET_INPUT: AtomicBool = AtomicBool::new(false);
static PAUSED: AtomicBool = AtomicBool::new(false);
static TRIGGER: AtomicU32 = AtomicU32::new(0xA3);
static MOUSE_BUTTONS: AtomicU32 = AtomicU32::new(0);
static POINTER: AtomicU64 = AtomicU64::new(0);
static POINTER_QUEUED: AtomicBool = AtomicBool::new(false);

thread_local! {static ROUTER:RefCell<orbit::input::Router>=RefCell::new(orbit::input::Router::new(0xA3));}
thread_local! {static EVENTS:RefCell<VecDeque<(u32,WPARAM,LPARAM)>>=const{RefCell::new(VecDeque::new())};}
fn post(msg: u32, w: usize) {
    unsafe {
        let _ = PostMessageW(
            Some(HWND(HOST.load(Relaxed) as _)),
            msg,
            WPARAM(w),
            LPARAM(0),
        );
    }
}

// Hooks only update small atomic state and queue messages. Drawing, file I/O and
// target-window operations run after dispatch, outside all hook callbacks.
unsafe extern "system" fn keyboard(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        if code < 0 {
            return CallNextHookEx(None, code, w, l);
        }
        let data = &*(l.0 as *const KBDLLHOOKSTRUCT);
        let state = STATE.load(Relaxed);
        let down = matches!(w.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
        let decision = ROUTER.with(|r| {
            let mut r = r.borrow_mut();
            if RESET_INPUT.swap(false, Relaxed) {
                r.reset();
            }
            r.set_trigger(TRIGGER.load(Relaxed));
            r.event(
                data.vkCode,
                down,
                data.flags.contains(LLKHF_INJECTED),
                PAUSED.load(Relaxed),
                state == PENDING,
                matches!(state, VISIBLE | STICKY),
            )
        });
        use orbit::input::Action;
        match decision.action {
            Some(Action::Arm) => {
                HELD.store(true, Relaxed);
                STATE.store(PENDING, Relaxed);
                post(MSG_BEGIN, 0);
            }
            Some(Action::Release) => {
                HELD.store(false, Relaxed);
                STATE.store(IDLE, Relaxed);
                post(MSG_END, 0);
            }
            Some(Action::Cancel) => {
                STATE.store(CANCELLED, Relaxed);
                post(MSG_CANCEL, 0);
            }
            Some(Action::Menu(key)) => {
                let shift = GetAsyncKeyState(VK_SHIFT.0 as i32) < 0;
                post(MSG_KEY, key as usize | ((shift as usize) << 16));
            }
            None => {}
        }
        if decision.consume {
            LRESULT(1)
        } else {
            CallNextHookEx(None, code, w, l)
        }
    }
}
unsafe extern "system" fn mouse(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        if code < 0 {
            return CallNextHookEx(None, code, w, l);
        }
        let data = &*(l.0 as *const MSLLHOOKSTRUCT);
        if data.flags & LLMHF_INJECTED != 0 {
            return CallNextHookEx(None, code, w, l);
        }
        let message = w.0 as u32;
        let button = match message {
            WM_LBUTTONDOWN | WM_LBUTTONUP => 1,
            WM_RBUTTONDOWN | WM_RBUTTONUP => 2,
            WM_MBUTTONDOWN | WM_MBUTTONUP => 4,
            WM_XBUTTONDOWN | WM_XBUTTONUP => {
                if data.mouseData >> 16 == 1 {
                    8
                } else {
                    16
                }
            }
            _ => 0,
        };
        let up = matches!(
            message,
            WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP | WM_XBUTTONUP
        );
        if up && MOUSE_BUTTONS.fetch_and(!button, Relaxed) & button != 0 {
            return LRESULT(1);
        }
        if matches!(STATE.load(Relaxed), VISIBLE | STICKY) {
            match message {
                WM_MOUSEMOVE => {
                    POINTER.store(
                        data.pt.x as u32 as u64 | ((data.pt.y as u32 as u64) << 32),
                        Relaxed,
                    );
                    if !POINTER_QUEUED.swap(true, Relaxed) {
                        post(MSG_POINTER, 0);
                    }
                }
                WM_MOUSEWHEEL => {
                    post(MSG_WHEEL, ((data.mouseData >> 16) as i16) as isize as usize);
                    return LRESULT(1);
                }
                WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN => {
                    MOUSE_BUTTONS.fetch_or(button, Relaxed);
                    STATE.store(CANCELLED, Relaxed);
                    post(MSG_CANCEL, 0);
                    return LRESULT(1);
                }
                _ => {}
            }
        }
        CallNextHookEx(None, code, w, l)
    }
}
unsafe extern "system" fn host_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_CLOSE => {
                PostQuitMessage(0);
                return LRESULT(0);
            }
            WM_ENDSESSION if w.0 != 0 => {
                PostQuitMessage(0);
                return LRESULT(0);
            }
            WM_QUERYENDSESSION => return LRESULT(1),
            _ => {
                EVENTS.with(|q| q.borrow_mut().push_back((msg, w, l)));
            }
        }
        DefWindowProcW(hwnd, msg, w, l)
    }
}

struct Hooks {
    thread: u32,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl Hooks {
    fn new() -> Result<Self> {
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || unsafe {
            let mut msg = MSG::default();
            let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
            let thread = GetCurrentThreadId();
            let installed = (|| -> Result<(HHOOK, HHOOK)> {
                let module = GetModuleHandleW(None).map_err(winerr)?.into();
                let key = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), Some(module), 0)
                    .map_err(winerr)?;
                match SetWindowsHookExW(WH_MOUSE_LL, Some(mouse), Some(module), 0) {
                    Ok(mouse) => Ok((key, mouse)),
                    Err(e) => {
                        let _ = UnhookWindowsHookEx(key);
                        Err(winerr(e))
                    }
                }
            })();
            match installed {
                Ok((key, mouse)) => {
                    if send.send(Ok(thread)).is_ok() {
                        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                            let _ = TranslateMessage(&msg);
                            DispatchMessageW(&msg);
                        }
                    }
                    let _ = UnhookWindowsHookEx(key);
                    let _ = UnhookWindowsHookEx(mouse);
                }
                Err(error) => {
                    let _ = send.send(Err(error));
                }
            }
        });
        let thread = receive
            .recv_timeout(std::time::Duration::from_secs(3))
            .map_err(|_| "Input hooks did not initialize.".to_string())??;
        Ok(Self {
            thread,
            worker: Some(worker),
        })
    }
}
impl Drop for Hooks {
    fn drop(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct Session {
    target: desktop::Snapshot,
    screens: Vec<desktop::Monitor>,
    monitor: usize,
    anchor: POINT,
    origin: desktop::Monitor,
    selection: Option<Layout>,
    variant: i32,
    keyboard_selected: bool,
    started: Instant,
    shown: bool,
    sticky: bool,
    wheel: i32,
}
struct App {
    hwnd: HWND,
    config: Config,
    overlays: overlay::Overlays,
    settings: settings::Settings,
    tray: shell::Tray,
    session: Option<Session>,
    undo: VecDeque<desktop::Snapshot>,
    verification: Option<(HWND, Rect)>,
    hotkeys: [bool; 2],
    taskbar_created: u32,
}
impl App {
    fn begin(&mut self, sticky: bool) -> Result<()> {
        self.cancel();
        if PAUSED.load(Relaxed) {
            return Ok(());
        }
        if unsafe {
            GetAsyncKeyState(VK_LBUTTON.0 as i32) < 0 || GetAsyncKeyState(VK_RBUTTON.0 as i32) < 0
        } {
            return Ok(());
        }
        let target = desktop::Snapshot::capture()?;
        let screens = desktop::monitors();
        let anchor = desktop::cursor()?;
        let monitor = screens
            .iter()
            .position(|m| m.work.contains(anchor.x, anchor.y))
            .or_else(|| screens.iter().position(|m| m.handle == target.monitor))
            .unwrap_or(0);
        let origin = *screens
            .get(monitor)
            .ok_or("No desktop monitor is available.")?;
        self.session = Some(Session {
            target,
            screens,
            monitor,
            anchor,
            origin,
            selection: None,
            variant: 0,
            keyboard_selected: false,
            started: Instant::now(),
            shown: sticky,
            sticky,
            wheel: 0,
        });
        STATE.store(if sticky { STICKY } else { PENDING }, Relaxed);
        unsafe {
            SetTimer(Some(self.hwnd), WATCHDOG, 40, None);
        }
        if sticky {
            self.draw()?;
        }
        Ok(())
    }
    fn cancel(&mut self) {
        self.session = None;
        self.overlays.hide();
        STATE.store(if HELD.load(Relaxed) { CANCELLED } else { IDLE }, Relaxed);
        unsafe {
            let _ = KillTimer(Some(self.hwnd), WATCHDOG);
        }
    }
    fn frame(&self) -> Option<Rect> {
        let s = self.session.as_ref()?;
        let layout = s.selection?;
        let display = s.screens[s.monitor];
        let original = if s.target.placement.showCmd == SW_SHOWMAXIMIZED.0 as u32 {
            s.target.restored
        } else {
            s.target.frame
        };
        Some(layout.frame(
            display.work,
            original,
            (self.config.gap as f64 * display.dpi as f64 / 96.).round() as i32,
            s.variant,
        ))
    }
    fn draw(&mut self) -> Result<()> {
        let frame = if self.config.preview {
            self.frame()
        } else {
            None
        };
        let result = if let Some(s) = &self.session {
            self.overlays.draw(overlay::Scene {
                anchor: s.anchor,
                work: s.origin.work,
                dpi: s.origin.dpi,
                selection: s.selection,
                variant: s.variant,
                frame,
                monitor: s.monitor,
                sticky: s.sticky,
            })
        } else {
            Ok(())
        };
        if result.is_err()
            && let Ok(recovered) = overlay::Overlays::new()
        {
            self.overlays = recovered;
        }
        result
    }
    fn finish(&mut self) -> Result<()> {
        if let Some(s) = &mut self.session
            && s.shown
            && !s.keyboard_selected
        {
            let p = desktop::cursor()?;
            s.selection = geometry::select(
                (p.x - s.anchor.x) as f64,
                (p.y - s.anchor.y) as f64,
                34. * s.origin.dpi as f64 / 96.,
                s.selection,
            );
        }
        let frame = self.frame();
        let session = self.session.take();
        self.cancel();
        if let (Some(s), Some(frame)) = (session, frame) {
            if !s.shown {
                return Ok(());
            }
            if !s.target.current_is_target() {
                return Err("The active window changed. No window was moved.".into());
            }
            if let Err(error) = s.target.apply(
                frame,
                s.selection == Some(Layout::Maximize) && s.variant.rem_euclid(3) == 0,
            ) {
                if s.target.current_is_target() {
                    let _ = s.target.undo();
                }
                return Err(error);
            }
            self.verification = Some((s.target.hwnd, frame));
            if self.undo.len() >= 32 {
                self.undo.pop_front();
            }
            self.undo.push_back(s.target);
            unsafe {
                SetTimer(Some(self.hwnd), VERIFY, 300, None);
            }
        }
        Ok(())
    }
    fn undo(&mut self) -> Result<()> {
        self.cancel();
        while let Some(snapshot) = self.undo.pop_back() {
            if snapshot.valid() {
                return snapshot.undo();
            }
        }
        Err("There is no window arrangement to undo yet.".into())
    }
    fn tick(&mut self) -> Result<()> {
        if let Some(s) = self.session.as_ref() {
            if !s.target.current_is_target() {
                self.cancel();
                return Ok(());
            }
            if !s.sticky && unsafe { GetAsyncKeyState(self.config.trigger as i32) >= 0 } {
                if HELD.swap(false, Relaxed) {
                    RESET_INPUT.store(true, Relaxed);
                }
                self.finish()?;
                return Ok(());
            }
            if !s.shown && s.started.elapsed().as_millis() >= self.config.delay_ms as u128 {
                if STATE.load(Relaxed) != PENDING {
                    self.cancel();
                    return Ok(());
                }
                self.session.as_mut().unwrap().shown = true;
                STATE.store(VISIBLE, Relaxed);
                if let Some(s) = &mut self.session {
                    let p = desktop::cursor()?;
                    s.selection = geometry::select(
                        (p.x - s.anchor.x) as f64,
                        (p.y - s.anchor.y) as f64,
                        34. * s.origin.dpi as f64 / 96.,
                        None,
                    );
                }
                self.draw()?;
            }
        }
        Ok(())
    }
    fn handle(&mut self, msg: u32, w: WPARAM, l: LPARAM) -> Result<()> {
        match msg {
            MSG_BEGIN => {
                if HELD.load(Relaxed) && STATE.load(Relaxed) == PENDING {
                    self.begin(false)?;
                }
            }
            MSG_END => {
                if self.session.as_ref().is_some_and(|s| !s.sticky) {
                    self.finish()?;
                }
            }
            MSG_CANCEL => self.cancel(),
            MSG_POINTER => {
                POINTER_QUEUED.store(false, Relaxed);
                let bits = POINTER.load(Relaxed);
                if let Some(s) = &mut self.session
                    && s.shown
                {
                    let next = geometry::select(
                        (bits as u32 as i32 - s.anchor.x) as f64,
                        ((bits >> 32) as u32 as i32 - s.anchor.y) as f64,
                        34. * s.origin.dpi as f64 / 96.,
                        s.selection,
                    );
                    if s.selection != next {
                        s.selection = next;
                        s.keyboard_selected = false;
                        s.variant = 0;
                        self.draw()?;
                    }
                }
            }
            MSG_WHEEL => {
                if let Some(s) = &mut self.session {
                    s.wheel += w.0 as isize as i32;
                    let steps = s.wheel / 120;
                    s.wheel %= 120;
                    if steps != 0 {
                        s.variant = (s.variant + steps).rem_euclid(3);
                        self.draw()?;
                    }
                }
            }
            MSG_KEY => match w.0 & 0xffff {
                0x1B => self.cancel(),
                0x0D => self.finish()?,
                0x5A => self.undo()?,
                0x09 => {
                    if let Some(s) = &mut self.session {
                        let dir = if w.0 >> 16 != 0 { -1 } else { 1 };
                        s.monitor =
                            (s.monitor as i32 + dir).rem_euclid(s.screens.len() as i32) as usize;
                        self.draw()?;
                    }
                }
                key => {
                    if let Some(s) = &mut self.session {
                        s.keyboard_selected = true;
                        s.selection = Some(match key {
                            0x25 | 0x34 => Layout::Left,
                            0x27 | 0x36 => Layout::Right,
                            0x28 | 0x32 | 0x35 => Layout::Center,
                            0x37 => Layout::TopLeft,
                            0x39 => Layout::TopRight,
                            0x31 => Layout::BottomLeft,
                            0x33 => Layout::BottomRight,
                            _ => Layout::Maximize,
                        });
                        s.variant = 0;
                        self.draw()?;
                    }
                }
            },
            WM_HOTKEY => {
                if w.0 == 1 {
                    if self.session.is_some() {
                        self.cancel();
                    } else {
                        self.begin(true)?;
                    }
                } else if w.0 == 2 {
                    self.undo()?;
                }
            }
            WM_TIMER if w.0 == WATCHDOG => self.tick()?,
            WM_TIMER if w.0 == VERIFY => {
                unsafe {
                    let _ = KillTimer(Some(self.hwnd), VERIFY);
                }
                if let Some((hwnd, expected)) = self.verification.take()
                    && let Ok(actual) = desktop::visible_frame(hwnd)
                    && ((actual.x - expected.x).abs() > 12
                        || (actual.y - expected.y).abs() > 12
                        || (actual.width - expected.width).abs() > 12
                        || (actual.height - expected.height).abs() > 12)
                {
                    self.tray.notify("This application adjusted the requested layout. Its minimum size or window style may limit snapping.");
                }
            }
            MSG_SHOW_SETTINGS => {
                self.cancel();
                self.settings.show();
            }
            MSG_TRAY => match l.0 as u32 {
                WM_LBUTTONUP | WM_LBUTTONDBLCLK => {
                    self.cancel();
                    self.settings.show();
                }
                WM_RBUTTONUP | WM_CONTEXTMENU => {
                    self.cancel();
                    match self.tray.menu(PAUSED.load(Relaxed)) {
                        1 => self.settings.show(),
                        2 => {
                            let paused = !PAUSED.load(Relaxed);
                            PAUSED.store(paused, Relaxed);
                            self.tray.update(
                                windows::Win32::UI::Shell::NIM_MODIFY,
                                if paused {
                                    "Orbit — paused"
                                } else {
                                    "Orbit — ready"
                                },
                            )?;
                        }
                        3 => self.undo()?,
                        4 => {
                            shell::elevate()?;
                            unsafe {
                                PostQuitMessage(0);
                            }
                        }
                        5 => unsafe {
                            PostQuitMessage(0);
                        },
                        _ => {}
                    }
                }
                _ => {}
            },
            MSG_SETTINGS => match w.0 {
                settings::SAVE => {
                    self.cancel();
                    let (config, startup) = self.settings.read();
                    config.save()?;
                    TRIGGER.store(config.trigger, Relaxed);
                    ROUTER.with(|r| r.borrow_mut().set_trigger(config.trigger));
                    self.config = config;
                    shell::set_startup(startup).map_err(|e| {
                        format!(
                            "Layout settings saved. Automatic startup could not be changed: {e}"
                        )
                    })?;
                    self.settings
                        .status("Saved. Orbit is ready in the notification area.");
                }
                settings::DIAGNOSE => {
                    let path = self.diagnostics()?;
                    shell::open_report(&path)?;
                }
                settings::INSTALL => {
                    let folder = shell::install()?;
                    self.settings.status(&format!(
                        "Installed in {}. Find Orbit in Start.",
                        folder.display()
                    ));
                }
                settings::FOLDER => {
                    std::fs::create_dir_all(Config::folder()).map_err(|e| e.to_string())?;
                    shell::open_path(&Config::folder())?;
                }
                999 => self.settings.layout(),
                2 => unsafe {
                    let _ = ShowWindow(self.settings.hwnd, SW_HIDE);
                },
                _ => {}
            },
            WM_DISPLAYCHANGE | WM_WTSSESSION_CHANGE | WM_POWERBROADCAST => {
                self.cancel();
                HELD.store(false, Relaxed);
                RESET_INPUT.store(true, Relaxed);
            }
            _ if msg == self.taskbar_created => {
                self.tray
                    .update(windows::Win32::UI::Shell::NIM_ADD, "Orbit — ready")?;
            }
            _ => {}
        }
        Ok(())
    }
    fn diagnostics(&self) -> Result<std::path::PathBuf> {
        let displays: Vec<_> = desktop::monitors()
            .iter()
            .map(|m| serde_json::json!({"work_area":m.work,"dpi":m.dpi}))
            .collect();
        let report = serde_json::json!({"application":"Orbit","version":env!("CARGO_PKG_VERSION"),"architecture":"x86_64",
            "checks":{"direct2d_initialized":true,"layered_windows_created":true,"keyboard_hook_installed":true,"mouse_hook_installed":true,
            "keyboard_menu_hotkey":self.hotkeys[0],"undo_hotkey":self.hotkeys[1],"per_monitor_v2":unsafe{AreDpiAwarenessContextsEqual(GetThreadDpiAwarenessContext(),DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2).as_bool()}},
            "elevated":desktop::elevated(unsafe{GetCurrentProcessId()}).unwrap_or(false),"startup_enabled":shell::startup_enabled(),"displays":displays,
            "privacy":"No keys, typed text, clipboard, application names, or window titles are recorded.",
            "scope":"Initialization checks only. Rendering, focus retention, layout acceptance and input behavior require interactive Windows testing."});
        let path = Config::folder().join("diagnostics.json");
        std::fs::create_dir_all(Config::folder()).map_err(|e| e.to_string())?;
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        Ok(path)
    }
}

pub fn run() -> Result<()> {
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED)
            .ok()
            .map_err(winerr)?;
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = InitCommonControlsEx(&INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_STANDARD_CLASSES,
        });
        let _ =
            windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(w!("Seffy.Orbit"));
        let args: Vec<_> = std::env::args().collect();
        if args.iter().any(|a| a == "--install") {
            let folder = shell::install()?;
            if args.iter().any(|a| a == "--desktop-shortcut") {
                shell::desktop_shortcut()?;
            }
            if args.iter().any(|a| a == "--start-with-windows") {
                shell::set_startup(true)?;
            }
            if !args.iter().any(|a| a == "--quiet") {
                MessageBoxW(None,PCWSTR(wide(&format!("Orbit is installed in {}.\nOpen Orbit from Start. Enable automatic startup in Settings if you want it.",folder.display())).as_ptr()),w!("Orbit installed"),MB_OK|MB_ICONINFORMATION);
            }
            return Ok(());
        }
        let mutex =
            CreateMutexW(None, false, w!("Local\\SeffyOrbitNativeInstance")).map_err(winerr)?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            if args.iter().any(|a| a == "--wait-for-exit") {
                for _ in 0..50 {
                    if FindWindowW(w!("OrbitHost"), None).is_err() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                if FindWindowW(w!("OrbitHost"), None).is_ok() {
                    return Err("The other Orbit instance has not exited yet.".into());
                }
            } else {
                if let Ok(host) = FindWindowW(w!("OrbitHost"), None) {
                    PostMessageW(
                        Some(host),
                        if args.iter().any(|a| a == "--uninstall") {
                            WM_CLOSE
                        } else if args.iter().any(|a| a == "--diagnose") {
                            MSG_SETTINGS
                        } else {
                            MSG_SHOW_SETTINGS
                        },
                        WPARAM(if args.iter().any(|a| a == "--diagnose") {
                            settings::DIAGNOSE
                        } else {
                            0
                        }),
                        LPARAM(0),
                    )
                    .map_err(winerr)?;
                }
                let _ = CloseHandle(mutex);
                if args.iter().any(|a| a == "--uninstall") {
                    shell::uninstall()?;
                }
                return Ok(());
            }
        }
        if args.iter().any(|a| a == "--uninstall") {
            shell::uninstall()?;
            return Ok(());
        }
        let instance = GetModuleHandleW(None).map_err(winerr)?;
        let wc = WNDCLASSW {
            lpfnWndProc: Some(host_proc),
            hInstance: instance.into(),
            lpszClassName: w!("OrbitHost"),
            ..Default::default()
        };
        if RegisterClassW(&wc) == 0 {
            return Err(winerr(Error::from_thread()));
        }
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("OrbitHost"),
            w!("Orbit"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .map_err(winerr)?;
        HOST.store(hwnd.0 as isize, Relaxed);
        let (config, config_error) = match Config::load() {
            Ok(config) => (config, None),
            Err(e) => (Config::default(), Some(e)),
        };
        TRIGGER.store(config.trigger, Relaxed);
        ROUTER.with(|r| r.borrow_mut().set_trigger(config.trigger));
        let overlays = overlay::Overlays::new()?;
        let icon = LoadIconW(Some(instance.into()), PCWSTR(1usize as _)).map_err(winerr)?;
        let tray = shell::Tray::new(hwnd, icon)?;
        let settings = settings::Settings::new(hwnd, &config, shell::startup_enabled())?;
        let hooks = Hooks::new()?;
        let hotkeys = [
            RegisterHotKey(
                Some(hwnd),
                1,
                MOD_WIN | MOD_ALT | MOD_NOREPEAT,
                VK_SPACE.0 as u32,
            )
            .is_ok(),
            RegisterHotKey(Some(hwnd), 2, MOD_WIN | MOD_ALT | MOD_NOREPEAT, 0x5A).is_ok(),
        ];
        let _ = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);
        let mut app = App {
            hwnd,
            config,
            overlays,
            settings,
            tray,
            session: None,
            undo: VecDeque::new(),
            verification: None,
            hotkeys,
            taskbar_created: RegisterWindowMessageW(w!("TaskbarCreated")),
        };
        if args.iter().any(|a| a == "--diagnose") {
            let path = app.diagnostics()?;
            shell::open_report(&path)?;
        } else {
            if app.config.first_run && !args.iter().any(|a| a == "--background") {
                app.settings.show();
            }
            if let Some(e) = config_error {
                app.settings.status(
                    "Settings could not be loaded. Defaults are active; Save repairs the file.",
                );
                shell::log(&format!("settings load: {e}"));
            }
            if !hotkeys.iter().all(|v| *v) {
                app.tray.notify("A keyboard shortcut is in use by another app. Hold the trigger key to use Orbit; see Diagnostics for details.");
            }
            shell::log("started");
            let mut msg = MSG::default();
            loop {
                let result = GetMessageW(&mut msg, None, 0, 0);
                if result.0 == -1 {
                    return Err(winerr(Error::from_thread()));
                }
                if result.0 == 0 {
                    break;
                }
                if !IsDialogMessageW(app.settings.hwnd, &msg).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                while let Some((m, w, l)) = EVENTS.with(|q| q.borrow_mut().pop_front()) {
                    if let Err(e) = app.handle(m, w, l) {
                        app.cancel();
                        shell::log(&e);
                        app.settings.status(&e);
                        app.tray.notify(&e);
                    }
                }
            }
        }
        app.cancel();
        drop(hooks);
        let _ = UnregisterHotKey(Some(hwnd), 1);
        let _ = UnregisterHotKey(Some(hwnd), 2);
        let _ = WTSUnRegisterSessionNotification(hwnd);
        drop(app);
        let _ = DestroyWindow(hwnd);
        let _ = CloseHandle(mutex);
        CoUninitialize();
        Ok(())
    }
}
pub fn fatal(error: &str) {
    shell::log(error);
    if std::env::args().any(|a| a == "--diagnose") {
        let folder = Config::folder();
        let _ = std::fs::create_dir_all(&folder);
        let path = folder.join("diagnostics.json");
        let report = serde_json::json!({"application":"Orbit","version":env!("CARGO_PKG_VERSION"),"initialization":"failed","error":error});
        if let Ok(data) = serde_json::to_vec_pretty(&report) {
            let _ = std::fs::write(&path, data);
            let _ = shell::open_report(&path);
        }
    }
    unsafe {
        MessageBoxW(None,PCWSTR(wide(&format!("Orbit could not continue.\n\n{error}\n\nDetails are in %LOCALAPPDATA%\\Orbit\\orbit.log.")).as_ptr()),w!("Orbit"),MB_OK|MB_ICONERROR);
    }
}
