#![allow(unsafe_op_in_unsafe_fn)]

use crate::motion::{Tear, tear_polygon};
use std::{cell::RefCell, path::PathBuf, ptr::null_mut, sync::mpsc, time::Instant};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{Gdi::*, GdiPlus as gp},
        Storage::FileSystem::{MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW},
        System::{Com::*, LibraryLoader::GetModuleHandleW, SystemServices::SS_CENTER},
        UI::{HiDpi::*, Input::KeyboardAndMouse::*, Shell::*, WindowsAndMessaging::*},
    },
    core::{BOOL, PCWSTR, w},
};

const PAYLOAD: &[u8] = include_bytes!(env!("ORBIT_PAYLOAD"));
const DONE: u32 = WM_APP + 1;
const ACTION: usize = 101;
const TIMER: usize = 1;
const BACKGROUND: COLORREF = COLORREF(0x00f8_f9fa); // COLORREF is BGR
const PAPER: (f32, f32, f32, f32) = (270., 132., 420., 210.);

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn check(status: gp::Status) -> Result<(), String> {
    if status == gp::Ok {
        Ok(())
    } else {
        Err(format!("Windows graphics error {}", status.0))
    }
}

struct Image {
    raw: *mut gp::GpImage,
    // GDI+ may decode lazily: the stream must outlive the image.
    _stream: IStream,
}
impl Image {
    unsafe fn load(bytes: &[u8]) -> Result<Self, String> {
        let stream = SHCreateMemStream(Some(bytes)).ok_or("לא ניתן לטעון את תמונת המתנה")?;
        let mut raw = null_mut();
        check(gp::GdipLoadImageFromStream(&stream, &mut raw))?;
        Ok(Self {
            raw,
            _stream: stream,
        })
    }
}
impl Drop for Image {
    fn drop(&mut self) {
        unsafe {
            gp::GdipDisposeImage(self.raw);
        }
    }
}
struct GdiStartup(usize);
impl Drop for GdiStartup {
    fn drop(&mut self) {
        unsafe {
            gp::GdiplusShutdown(self.0);
        }
    }
}

enum Phase {
    Wrapped,
    Opening { start: Instant, from: f32 },
    Configure,
    Installing,
    Ready(PathBuf),
    Failed(String),
}
struct State {
    tear: Tear,
    phase: Phase,
    paper: Image,
    product: Image,
    flap: Image,
    dedication: Image,
    controls: [HWND; 6], // accessible native headline, subtitle, button, disclosure
    fonts: Vec<HFONT>,
    result: Option<mpsc::Receiver<Result<PathBuf, String>>>,
    reduce_motion: bool,
    scale: f32,
    background: HBRUSH,
}
impl Drop for State {
    fn drop(&mut self) {
        unsafe {
            for font in &self.fonts {
                let _ = DeleteObject((*font).into());
            }
            let _ = DeleteObject(self.background.into());
        }
    }
}
thread_local! { static STATE: RefCell<Option<State>> = const { RefCell::new(None) }; }

pub fn show_error(error: &str) {
    unsafe {
        MessageBoxW(
            None,
            PCWSTR(wide(error).as_ptr()),
            w!("Orbit"),
            MB_OK | MB_ICONERROR,
        );
    }
}

unsafe fn set_text(hwnd: HWND, text: &str) {
    let _ = SetWindowTextW(hwnd, PCWSTR(wide(text).as_ptr()));
}
unsafe fn font(size: f32, weight: i32, scale: f32) -> HFONT {
    CreateFontW(
        -(size * scale).round() as i32,
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        DEFAULT_CHARSET,
        OUT_DEFAULT_PRECIS,
        CLIP_DEFAULT_PRECIS,
        CLEARTYPE_QUALITY,
        DEFAULT_PITCH.0 as u32,
        w!("Segoe UI"),
    )
}
unsafe fn layout(hwnd: HWND, state: &mut State) {
    let mut rect = RECT::default();
    let _ = GetClientRect(hwnd, &mut rect);
    state.scale = rect.right as f32 / 960.;
    let s = state.scale;
    let h = rect.bottom as f32 / s;
    let boxes = [
        (40., 30., 880., 61.),
        (40., 96., 880., 43.),
        (270., h - 85., 420., 43.),
        (30., h - 31., 900., 25.),
        (490., h - 131., 270., 30.),
        (180., h - 131., 300., 30.),
    ];
    for (ctrl, (x, y, w, h)) in state.controls.iter().zip(boxes) {
        let _ = MoveWindow(
            *ctrl,
            (x * s) as i32,
            (y * s) as i32,
            (w * s) as i32,
            (h * s) as i32,
            true,
        );
    }
    for f in state.fonts.drain(..) {
        let _ = DeleteObject(f.into());
    }
    for (ctrl, (size, weight)) in state.controls.iter().zip([
        (40., 800),
        (21., 400),
        (21., 600),
        (13., 400),
        (17., 400),
        (17., 400),
    ]) {
        let f = font(size, weight, s);
        SendMessageW(
            *ctrl,
            WM_SETFONT,
            Some(WPARAM(f.0 as usize)),
            Some(LPARAM(1)),
        );
        state.fonts.push(f);
    }
    state.fonts.push(font(13., 400, s));
    refresh(state);
}

unsafe fn refresh(state: &State) {
    let (headline, subtitle, action, footer, enabled) = match &state.phase {
        Phase::Wrapped | Phase::Opening { .. } => (
            "כל החלונות. כל הבלגן.",
            "משוך את הנייר מימין לשמאל כדי לעשות קצת סדר",
            "או אפשר גם להתקין בלי להרוס את עטיפת המתנה",
            "מספי · פותחים, בוחרים ומתקינים · גרסת ניסיון לא חתומה",
            matches!(state.phase, Phase::Wrapped),
        ),
        Phase::Configure => (
            "קצת יותר סדר. קצת פחות גרירות.",
            "Orbit מסדר חלונות בתנועה אחת. נתקין?",
            "התקנה",
            "Windows 10/11 x64 · התקנה למשתמש הנוכחי, ללא הרשאות מנהל או הורדות נוספות",
            true,
        ),
        Phase::Installing => (
            "עוד רגע, והיא שלך",
            "מתקינים את Orbit במחשב הזה…",
            "מתקינים…",
            "כל העיבוד מקומי. ללא הורדות נוספות.",
            false,
        ),
        Phase::Ready(_) => (
            "המתנה שלך מוכנה",
            "מחזיקים Ctrl ימני, מזיזים לכיוון הרצוי ומשחררים.",
            "פתח את Orbit",
            "אפשר גם Win + Alt + Space · ההגדרות נפתחות בהפעלה הראשונה · תתחדש. מספי",
            true,
        ),
        Phase::Failed(message) => (
            "כמעט שם",
            message.as_str(),
            "נסה שוב",
            "אם Orbit כבר פתוח, סגור אותו ונסה שוב. ההגדרות הקיימות נשמרות.",
            true,
        ),
    };
    for (ctrl, text) in state
        .controls
        .iter()
        .zip([headline, subtitle, action, footer])
    {
        set_text(*ctrl, text);
    }
    let _ = EnableWindow(state.controls[2], enabled);
    let button_font = if matches!(state.phase, Phase::Wrapped | Phase::Opening { .. }) {
        6
    } else {
        2
    };
    if let Some(f) = state.fonts.get(button_font) {
        SendMessageW(
            state.controls[2],
            WM_SETFONT,
            Some(WPARAM(f.0 as usize)),
            Some(LPARAM(1)),
        );
    }
    for control in &state.controls[4..] {
        let _ = ShowWindow(
            *control,
            if matches!(state.phase, Phase::Configure) {
                SW_SHOW
            } else {
                SW_HIDE
            },
        );
    }
}

unsafe fn start_install(hwnd: HWND, state: &mut State) {
    if matches!(state.phase, Phase::Installing | Phase::Ready(_)) {
        return;
    }
    let desktop = SendMessageW(state.controls[4], BM_GETCHECK, None, None).0 == 1;
    let startup = SendMessageW(state.controls[5], BM_GETCHECK, None, None).0 == 1;
    state.tear.cancel();
    state.tear.progress = 1.;
    state.phase = Phase::Installing;
    refresh(state);
    let (tx, rx) = mpsc::channel();
    state.result = Some(rx);
    let handle = hwnd.0 as usize;
    std::thread::spawn(move || {
        let result = install_payload(desktop, startup);
        let _ = tx.send(result);
        unsafe {
            let _ = PostMessageW(Some(HWND(handle as *mut _)), DONE, WPARAM(0), LPARAM(0));
        }
    });
    let _ = InvalidateRect(Some(hwnd), None, false);
}
unsafe fn reveal(hwnd: HWND, state: &mut State) {
    state.tear.cancel();
    state.tear.progress = 1.;
    state.phase = Phase::Configure;
    refresh(state);
    let _ = SetFocus(Some(state.controls[4]));
    let _ = InvalidateRect(Some(hwnd), None, false);
}
unsafe fn finish_tear(hwnd: HWND, state: &mut State) {
    if state.reduce_motion {
        reveal(hwnd, state);
    } else {
        state.phase = Phase::Opening {
            start: Instant::now(),
            from: state.tear.progress,
        };
        refresh(state);
        SetTimer(Some(hwnd), TIMER, 16, None);
    }
}

unsafe fn draw_image(g: *mut gp::GpGraphics, image: &Image, x: f32, y: f32, w: f32, h: f32) {
    gp::GdipDrawImageRect(g, image.raw, x, y, w, h);
}

unsafe fn paint(hwnd: HWND, state: &State) {
    let mut ps = PAINTSTRUCT::default();
    let dc = BeginPaint(hwnd, &mut ps);
    let mut r = RECT::default();
    let _ = GetClientRect(hwnd, &mut r);
    if r.right <= 0 || r.bottom <= 0 {
        let _ = EndPaint(hwnd, &ps);
        return;
    }
    let buffer = CreateCompatibleDC(Some(dc));
    let bitmap = CreateCompatibleBitmap(dc, r.right, r.bottom);
    let old = SelectObject(buffer, bitmap.into());
    FillRect(buffer, &r, state.background);
    let mut g = null_mut();
    if gp::GdipCreateFromHDC(buffer, &mut g) == gp::Ok {
        let s = state.scale;
        gp::GdipScaleWorldTransform(g, s, s, gp::MatrixOrderAppend);
        gp::GdipSetSmoothingMode(g, gp::SmoothingModeAntiAlias);
        gp::GdipSetInterpolationMode(g, gp::InterpolationModeHighQualityBicubic);
        let (x, y, w, h) = PAPER;
        let paper_scale = w / 840.;
        draw_image(
            g,
            &state.product,
            x + 8. * paper_scale,
            y + 5. * paper_scale,
            w - 16. * paper_scale,
            h - 8. * paper_scale,
        );
        draw_image(
            g,
            &state.dedication,
            x + w * 0.58,
            y + h * 0.48,
            270. * paper_scale,
            180. * paper_scale,
        );
        if matches!(state.phase, Phase::Wrapped | Phase::Opening { .. }) && state.tear.progress < 1.
        {
            let p = state.tear.progress;
            let points: Vec<gp::PointF> = tear_polygon(p)
                .into_iter()
                .map(|(px, py)| gp::PointF {
                    X: x + px * w,
                    Y: y + py * h,
                })
                .collect();
            if points.is_empty() {
                draw_image(g, &state.paper, x, y, w, h);
            } else {
                let mut path = null_mut();
                if gp::GdipCreatePath(gp::FillModeAlternate, &mut path) == gp::Ok {
                    gp::GdipAddPathPolygon(path, points.as_ptr(), points.len() as i32);
                    let mut saved = 0;
                    gp::GdipSaveGraphics(g, &mut saved);
                    gp::GdipSetClipPath(g, path, gp::CombineModeExclude);
                    draw_image(g, &state.paper, x, y, w, h);
                    gp::GdipRestoreGraphics(g, saved);
                    let mut pen = null_mut();
                    if gp::GdipCreatePen1(0xffeeeeec, 2.2 * paper_scale, gp::UnitPixel, &mut pen)
                        == gp::Ok
                    {
                        // Fibers belong on the paper, not in the transparent margin.
                        gp::GdipSaveGraphics(g, &mut saved);
                        gp::GdipSetClipRect(
                            g,
                            x + 18. * paper_scale,
                            y + h * 0.165,
                            w - 36. * paper_scale,
                            h * 0.78,
                            gp::CombineModeIntersect,
                        );
                        gp::GdipDrawPath(g, pen, path);
                        gp::GdipRestoreGraphics(g, saved);
                        gp::GdipDeletePen(pen);
                    }
                    gp::GdipDeletePath(path);
                }
            }
            if p < 0.85 {
                let mut saved = 0;
                gp::GdipSaveGraphics(g, &mut saved);
                let tip_x = x + (1. - p * 1.14).max(0.07) * w;
                let tip_y = y + (0.82 - (1. - p * 1.14) * 0.62) * h;
                gp::GdipTranslateWorldTransform(g, tip_x, tip_y, gp::MatrixOrderPrepend);
                gp::GdipRotateWorldTransform(g, -25. + p * 35., gp::MatrixOrderPrepend);
                let fw = (if p < 0.02 { 145. } else { 185. + 120. * p }) * paper_scale;
                draw_image(g, &state.flap, -fw * 0.6, -fw * 0.32, fw, fw * 0.5);
                gp::GdipRestoreGraphics(g, saved);
            }
        }
        gp::GdipDeleteGraphics(g);
    }
    let _ = BitBlt(dc, 0, 0, r.right, r.bottom, Some(buffer), 0, 0, SRCCOPY);
    SelectObject(buffer, old);
    let _ = DeleteObject(bitmap.into());
    let _ = DeleteDC(buffer);
    let _ = EndPaint(hwnd, &ps);
}

fn mouse(lp: LPARAM, scale: f32) -> (f32, f32) {
    (
        ((lp.0 as u32 & 0xffff) as i16) as f32 / scale,
        (((lp.0 as u32 >> 16) & 0xffff) as i16) as f32 / scale,
    )
}
unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_DESTROY {
        let _ = KillTimer(Some(hwnd), TIMER);
        STATE.with(|slot| {
            if let Ok(mut slot) = slot.try_borrow_mut() {
                slot.take();
            }
        });
        PostQuitMessage(0);
        return LRESULT(0);
    }
    let handled = STATE.with(|slot| {
        let Ok(mut slot) = slot.try_borrow_mut() else {
            return None;
        };
        let state = slot.as_mut()?;
        match msg {
            WM_ERASEBKGND => Some(LRESULT(1)),
            WM_PAINT => {
                paint(hwnd, state);
                Some(LRESULT(0))
            }
            WM_CTLCOLORSTATIC => {
                let dc = HDC(wp.0 as *mut _);
                SetTextColor(
                    dc,
                    if HWND(lp.0 as *mut _) == state.controls[0] {
                        COLORREF(0x00271811)
                    } else {
                        COLORREF(0x00766152)
                    },
                );
                SetBkColor(dc, BACKGROUND);
                Some(LRESULT(state.background.0 as isize))
            }
            WM_SIZE => {
                layout(hwnd, state);
                let _ = InvalidateRect(Some(hwnd), None, false);
                Some(LRESULT(0))
            }
            WM_DPICHANGED => {
                let r = &*(lp.0 as *const RECT);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                layout(hwnd, state);
                Some(LRESULT(0))
            }
            WM_LBUTTONDOWN if matches!(state.phase, Phase::Wrapped) => {
                let (x, y) = mouse(lp, state.scale);
                let (px, py, w, h) = PAPER;
                if x >= px && x <= px + w && y >= py && y <= py + h {
                    state.tear.begin(x, y);
                    SetCapture(hwnd);
                    let _ = SetFocus(Some(hwnd));
                }
                Some(LRESULT(0))
            }
            WM_MOUSEMOVE if state.tear.dragging() => {
                let (x, y) = mouse(lp, state.scale);
                state.tear.update(x, y, PAPER.2);
                let _ = InvalidateRect(Some(hwnd), None, false);
                Some(LRESULT(0))
            }
            WM_LBUTTONUP if state.tear.dragging() => {
                let complete = state.tear.release();
                let _ = ReleaseCapture();
                if complete {
                    finish_tear(hwnd, state);
                }
                Some(LRESULT(0))
            }
            WM_CAPTURECHANGED | WM_CANCELMODE | WM_KILLFOCUS => {
                state.tear.cancel();
                Some(LRESULT(0))
            }
            WM_KEYDOWN if wp.0 == VK_ESCAPE.0 as usize => {
                state.tear.cancel();
                let _ = ReleaseCapture();
                Some(LRESULT(0))
            }
            WM_KEYDOWN if wp.0 == VK_RETURN.0 as usize || wp.0 == VK_SPACE.0 as usize => {
                let _ = PostMessageW(Some(hwnd), WM_COMMAND, WPARAM(ACTION), LPARAM(0));
                Some(LRESULT(0))
            }
            WM_COMMAND if wp.0 & 0xffff == ACTION => {
                match &state.phase {
                    Phase::Ready(path) => match std::process::Command::new(path).spawn() {
                        Ok(_) => {
                            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                        }
                        Err(e) => {
                            state.phase = Phase::Failed(format!("לא ניתן לפתוח את Orbit: {e}"));
                            refresh(state);
                        }
                    },
                    Phase::Wrapped => reveal(hwnd, state),
                    Phase::Configure | Phase::Failed(_) => start_install(hwnd, state),
                    _ => (),
                }
                Some(LRESULT(0))
            }
            WM_TIMER if wp.0 == TIMER => {
                if let Phase::Opening { start, from } = &state.phase {
                    let t = (start.elapsed().as_secs_f32() / 0.3).min(1.);
                    state.tear.progress = *from + (1. - *from) * (1. - (1. - t).powi(3));
                    if t >= 1. {
                        let _ = KillTimer(Some(hwnd), TIMER);
                        reveal(hwnd, state);
                    }
                    let _ = InvalidateRect(Some(hwnd), None, false);
                } else {
                    let _ = KillTimer(Some(hwnd), TIMER);
                }
                Some(LRESULT(0))
            }
            DONE => {
                if let Some(result) = state.result.as_ref().and_then(|r| r.try_recv().ok()) {
                    state.phase = match result {
                        Ok(path) => Phase::Ready(path),
                        Err(e) => Phase::Failed(e),
                    };
                    state.result = None;
                    refresh(state);
                    let _ = SetFocus(Some(state.controls[2]));
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                Some(LRESULT(0))
            }
            WM_CLOSE if matches!(state.phase, Phase::Installing) => Some(LRESULT(0)),
            _ => None,
        }
    });
    handled.unwrap_or_else(|| DefWindowProcW(hwnd, msg, wp, lp))
}

pub fn run() -> Result<(), String> {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let mut token = 0;
        check(gp::GdiplusStartup(
            &mut token,
            &gp::GdiplusStartupInput {
                GdiplusVersion: 1,
                SuppressExternalCodecs: true.into(),
                ..Default::default()
            },
            null_mut(),
        ))?;
        let _startup = GdiStartup(token);
        let paper = Image::load(include_bytes!("../assets/wrapped-parcel.png"))?;
        let product = Image::load(include_bytes!("../assets/product-box.png"))?;
        let flap = Image::load(include_bytes!("../assets/paper-flap.png"))?;
        let dedication = Image::load(include_bytes!("../assets/dedication.png"))?;
        let instance = HINSTANCE(GetModuleHandleW(None).map_err(err)?.0);
        let class = w!("OrbitGiftInstaller");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: instance,
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW).map_err(err)?,
            // MAKEINTRESOURCEW(1): this value is a resource ID, never dereferenced.
            hIcon: LoadIconW(Some(instance), PCWSTR(std::ptr::without_provenance(1)))
                .unwrap_or_default(),
            ..Default::default()
        };
        if RegisterClassW(&wc) == 0 {
            return Err("לא ניתן לפתוח את מתקין Orbit".into());
        }
        let mut work = RECT::default();
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some((&mut work as *mut RECT).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
        .map_err(err)?;
        let scale = (GetDpiForSystem() as f32 / 96.)
            .min((work.right - work.left - 32) as f32 / 980.)
            .min((work.bottom - work.top - 32) as f32 / 570.);
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN;
        let mut r = RECT {
            left: 0,
            top: 0,
            right: (960. * scale) as i32,
            bottom: (520. * scale) as i32,
        };
        AdjustWindowRectEx(&mut r, style, false, WINDOW_EX_STYLE(0)).map_err(err)?;
        let width = r.right - r.left;
        let height = r.bottom - r.top;
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            w!("Orbit · מתנה מספי"),
            style,
            work.left + ((work.right - work.left) - width) / 2,
            work.top + ((work.bottom - work.top) - height) / 2,
            width,
            height,
            None,
            None,
            Some(instance),
            None,
        )
        .map_err(err)?;
        let make_control =
            |class: PCWSTR, id: usize, extra: WINDOW_STYLE| -> Result<HWND, String> {
                CreateWindowExW(
                    WS_EX_RTLREADING,
                    class,
                    w!(""),
                    WS_CHILD | WS_VISIBLE | extra,
                    0,
                    0,
                    1,
                    1,
                    Some(hwnd),
                    Some(HMENU(id as *mut _)),
                    Some(instance),
                    None,
                )
                .map_err(err)
            };
        let controls = [
            make_control(w!("STATIC"), 0, WINDOW_STYLE(SS_CENTER.0))?,
            make_control(w!("STATIC"), 0, WINDOW_STYLE(SS_CENTER.0))?,
            make_control(
                w!("BUTTON"),
                ACTION,
                WS_TABSTOP | WINDOW_STYLE(BS_DEFPUSHBUTTON as u32),
            )?,
            make_control(w!("STATIC"), 0, WINDOW_STYLE(SS_CENTER.0))?,
            make_control(
                w!("BUTTON"),
                102,
                WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            )?,
            make_control(
                w!("BUTTON"),
                103,
                WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            )?,
        ];
        set_text(controls[4], "קיצור על שולחן העבודה");
        set_text(controls[5], "הפעלה עם כניסה ל־Windows");
        let mut animation = BOOL(1);
        let _ = SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&mut animation as *mut BOOL).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
        let mut state = State {
            tear: Tear::default(),
            phase: Phase::Wrapped,
            paper,
            product,
            flap,
            dedication,
            controls,
            fonts: Vec::new(),
            result: None,
            reduce_motion: !animation.as_bool(),
            scale,
            background: CreateSolidBrush(BACKGROUND),
        };
        layout(hwnd, &mut state);
        refresh(&state);
        STATE.with(|slot| *slot.borrow_mut() = Some(state));
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetFocus(Some(controls[2]));
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            // Native controls support Tab and Narrator; Enter/Space also work without dragging.
            if !IsDialogMessageW(hwnd, &msg).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        Ok(())
    }
}

/// Write only the app payload, never the gift bootstrap. Replace atomically;
/// failure (e.g. a running old exe) leaves the old app and its config intact.
fn install_payload(desktop: bool, startup: bool) -> Result<PathBuf, String> {
    use std::{io::Write, os::windows::process::CommandExt};
    unsafe {
        validate_payload(PAYLOAD)?;
        let root = PathBuf::from(
            std::env::var_os("LOCALAPPDATA").ok_or("לא ניתן למצוא את תיקיית המשתמש")?,
        )
        .join("Programs/Orbit");
        std::fs::create_dir_all(&root).map_err(err)?;
        let destination = root.join("Orbit.exe");
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(err)?
            .as_nanos();
        let temporary = root.join(format!("Orbit-{}-{nonce}.tmp", std::process::id()));
        {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(err)?;
            if let Err(e) = file.write_all(PAYLOAD).and_then(|_| file.sync_all()) {
                drop(file);
                let _ = std::fs::remove_file(&temporary);
                return Err(err(e));
            }
        }
        if let Err(e) = MoveFileExW(
            PCWSTR(wide(&temporary.to_string_lossy()).as_ptr()),
            PCWSTR(wide(&destination.to_string_lossy()).as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        ) {
            let _ = std::fs::remove_file(&temporary);
            return Err(format!("סגור את Orbit ונסה שוב. {e}"));
        }
        for (name, bytes) in [
            ("LICENSE.txt", include_bytes!("../../LICENSE").as_slice()),
            (
                "THIRD-PARTY-NOTICES.txt",
                include_bytes!("../../THIRD-PARTY-NOTICES.txt").as_slice(),
            ),
            (
                "Readme.txt",
                include_bytes!("../../packaging/gift-readme-he.txt").as_slice(),
            ),
            (
                "Check-Windows-Startup.cmd",
                include_bytes!("../../packaging/Check-Windows-Startup.cmd").as_slice(),
            ),
        ] {
            std::fs::write(root.join(name), bytes).map_err(err)?;
        }
        let mut command = std::process::Command::new(&destination);
        command.args(["--install", "--quiet"]);
        if desktop {
            command.arg("--desktop-shortcut");
        }
        if startup {
            command.arg("--start-with-windows");
        }
        let result = command.creation_flags(0x08000000).status().map_err(err)?;
        if !result.success() {
            return Err("קובצי Orbit הועתקו, אבל רישום ההתקנה לא הושלם. נסה שוב או הרץ Check-Windows-Startup.cmd בתיקיית Orbit.".into());
        }
        Ok(destination)
    }
}

fn validate_payload(bytes: &[u8]) -> Result<(), String> {
    let invalid = || "חבילת ההתקנה אינה תקינה. יש להוריד אותה מחדש.".to_string();
    if bytes.len() < 64 || &bytes[..2] != b"MZ" {
        return Err(invalid());
    }
    let offset = u32::from_le_bytes(bytes[60..64].try_into().map_err(|_| invalid())?) as usize;
    if bytes.get(offset..offset + 6) != Some(b"PE\0\0\x64\x86".as_slice()) {
        return Err(invalid());
    }
    Ok(())
}
