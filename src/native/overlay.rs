use super::{Result, winerr};
use orbit::geometry::{Layout, Rect};
use std::{f32::consts::PI, mem::size_of};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{
            Direct2D::{Common::*, *},
            DirectWrite::*,
            Dxgi::Common::*,
            Gdi::*,
        },
        System::LibraryLoader::*,
        UI::WindowsAndMessaging::*,
    },
    core::*,
};
use windows_numerics::Vector2;

struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    old: HGDIOBJ,
    width: i32,
    height: i32,
}
impl Surface {
    fn new(width: i32, height: i32) -> Result<Self> {
        if !(1..=16384).contains(&width) || !(1..=16384).contains(&height) {
            return Err("Overlay size is outside the supported display range.".into());
        }
        unsafe {
            let dc = CreateCompatibleDC(None);
            if dc.is_invalid() {
                return Err("Windows could not create a drawing surface.".into());
            }
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            let bitmap = match CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0)
            {
                Ok(bitmap) => bitmap,
                Err(e) => {
                    let _ = DeleteDC(dc);
                    return Err(winerr(e));
                }
            };
            let old = SelectObject(dc, bitmap.into());
            Ok(Self {
                dc,
                bitmap,
                old,
                width,
                height,
            })
        }
    }
}
impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.dc);
        }
    }
}

pub struct Layer {
    pub hwnd: HWND,
    surface: Option<Surface>,
}
impl Layer {
    fn new() -> Result<Self> {
        unsafe {
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                w!("OrbitOverlay"),
                w!("Orbit preview"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(GetModuleHandleW(None).map_err(winerr)?.into()),
                None,
            )
            .map_err(winerr)?;
            Ok(Self {
                hwnd,
                surface: None,
            })
        }
    }
    pub fn hide(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }
    fn prepare(&mut self, width: i32, height: i32) -> Result<&Surface> {
        if self
            .surface
            .as_ref()
            .is_none_or(|s| s.width != width || s.height != height)
        {
            self.surface = Some(Surface::new(width, height)?);
        }
        Ok(self.surface.as_ref().unwrap())
    }
    fn present(&self, x: i32, y: i32) -> Result<()> {
        let s = self.surface.as_ref().ok_or("Missing overlay surface")?;
        unsafe {
            UpdateLayeredWindow(
                self.hwnd,
                None,
                Some(&POINT { x, y }),
                Some(&SIZE {
                    cx: s.width,
                    cy: s.height,
                }),
                Some(s.dc),
                Some(&POINT::default()),
                COLORREF(0),
                Some(&BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: 255,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                }),
                ULW_ALPHA,
            )
            .map_err(winerr)?;
            SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
            .map_err(winerr)
        }
    }
}
impl Drop for Layer {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

pub struct Overlays {
    pub ring: Layer,
    pub preview: Layer,
    factory: ID2D1Factory,
    target: ID2D1DCRenderTarget,
    write: IDWriteFactory,
}
pub struct Scene {
    pub anchor: POINT,
    pub work: Rect,
    pub dpi: u32,
    pub selection: Option<Layout>,
    pub variant: i32,
    pub frame: Option<Rect>,
    pub monitor: usize,
    pub sticky: bool,
}

impl Overlays {
    pub fn new() -> Result<Self> {
        unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
            match msg {
                WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
                WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
                _ => unsafe { DefWindowProcW(hwnd, msg, w, l) },
            }
        }
        unsafe {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(proc),
                hInstance: GetModuleHandleW(None).map_err(winerr)?.into(),
                lpszClassName: w!("OrbitOverlay"),
                ..Default::default()
            };
            if RegisterClassW(&wc) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
                return Err(winerr(Error::from_thread()));
            }
            let factory: ID2D1Factory =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None).map_err(winerr)?;
            let target = factory
                .CreateDCRenderTarget(&D2D1_RENDER_TARGET_PROPERTIES {
                    r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                    },
                    dpiX: 96.,
                    dpiY: 96.,
                    usage: D2D1_RENDER_TARGET_USAGE_NONE,
                    minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
                })
                .map_err(winerr)?;
            target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            Ok(Self {
                ring: Layer::new()?,
                preview: Layer::new()?,
                factory,
                target,
                write: DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).map_err(winerr)?,
            })
        }
    }
    pub fn hide(&self) {
        self.ring.hide();
        self.preview.hide();
    }
    pub fn draw(&mut self, scene: Scene) -> Result<()> {
        let Scene {
            anchor,
            work,
            dpi,
            selection,
            variant,
            frame,
            monitor,
            sticky,
        } = scene;
        let s = dpi as f32 / 96.;
        let (width, height) = ((304. * s).round() as i32, (354. * s).round() as i32);
        let x = (anchor.x - width / 2).clamp(work.x, (work.right() - width).max(work.x));
        let y = (anchor.y - (144. * s) as i32).clamp(work.y, (work.bottom() - height).max(work.y));
        unsafe {
            let surface = self.ring.prepare(width, height)?;
            self.target
                .BindDC(
                    surface.dc,
                    &RECT {
                        left: 0,
                        top: 0,
                        right: width,
                        bottom: height,
                    },
                )
                .map_err(winerr)?;
            self.target.BeginDraw();
            self.target.Clear(Some(&color(0, 0, 0, 0.)));
            let dark = self
                .target
                .CreateSolidColorBrush(&color(19, 24, 34, 0.97), None)
                .map_err(winerr)?;
            let line = self
                .target
                .CreateSolidColorBrush(&color(93, 112, 137, 0.55), None)
                .map_err(winerr)?;
            let accent = self
                .target
                .CreateSolidColorBrush(&color(109, 232, 209, 1.), None)
                .map_err(winerr)?;
            let pale = self
                .target
                .CreateSolidColorBrush(&color(232, 239, 247, 1.), None)
                .map_err(winerr)?;
            let muted = self
                .target
                .CreateSolidColorBrush(&color(163, 179, 198, 1.), None)
                .map_err(winerr)?;
            for (i, layout) in Layout::ALL.iter().enumerate() {
                let selected = selection == Some(*layout);
                let geometry = self.factory.CreatePathGeometry().map_err(winerr)?;
                let sink = geometry.Open().map_err(winerr)?;
                let angle = i as f32 * PI / 4. - PI / 2.;
                let start = angle - PI / 8. + 0.026;
                let end = angle + PI / 8. - 0.026;
                let p = |a: f32, r: f32| Vector2 {
                    X: (152. + a.cos() * r) * s,
                    Y: (144. + a.sin() * r) * s,
                };
                sink.BeginFigure(p(start, 124.), D2D1_FIGURE_BEGIN_FILLED);
                for n in 1..=24 {
                    sink.AddLine(p(start + (end - start) * n as f32 / 24., 124.));
                }
                sink.AddLine(p(end, 70.));
                for n in (0..24).rev() {
                    sink.AddLine(p(start + (end - start) * n as f32 / 24., 70.));
                }
                sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                sink.Close().map_err(winerr)?;
                self.target
                    .FillGeometry(&geometry, if selected { &accent } else { &dark }, None);
                let ink = if selected { &dark } else { &pale };
                let cx = 152. + angle.cos() * 96.;
                let cy = 144. + angle.sin() * 96.;
                let outer = round(cx - 12., cy - 9., cx + 12., cy + 9., 3., s);
                self.target.DrawRoundedRectangle(&outer, ink, 1.6 * s, None);
                let (ix, iy, iw, ih) = match layout {
                    Layout::Maximize => (cx - 8., cy - 5., 16., 10.),
                    Layout::Center => (cx - 5., cy - 3., 10., 6.),
                    Layout::Left => (cx - 9., cy - 6., 7., 12.),
                    Layout::Right => (cx + 2., cy - 6., 7., 12.),
                    Layout::TopLeft => (cx - 9., cy - 6., 7., 4.),
                    Layout::TopRight => (cx + 2., cy - 6., 7., 4.),
                    Layout::BottomLeft => (cx - 9., cy + 2., 7., 4.),
                    Layout::BottomRight => (cx + 2., cy + 2., 7., 4.),
                };
                self.target
                    .FillRoundedRectangle(&round(ix, iy, ix + iw, iy + ih, 1., s), ink);
            }
            self.target.FillEllipse(
                &D2D1_ELLIPSE {
                    point: Vector2 {
                        X: 152. * s,
                        Y: 144. * s,
                    },
                    radiusX: 59. * s,
                    radiusY: 59. * s,
                },
                &dark,
            );
            self.target.DrawEllipse(
                &D2D1_ELLIPSE {
                    point: Vector2 {
                        X: 152. * s,
                        Y: 144. * s,
                    },
                    radiusX: 58.5 * s,
                    radiusY: 58.5 * s,
                },
                &line,
                s,
                None,
            );
            let label = selection
                .map(|v| v.label(variant))
                .unwrap_or("Move to select");
            self.text(label, 14. * s, box_rect(100., 119., 204., 161., s), &pale)?;
            self.text(
                &format!("DISPLAY {}", monitor + 1),
                8.5 * s,
                box_rect(99., 164., 205., 180., s),
                &muted,
            )?;
            self.target
                .FillRoundedRectangle(&round(28., 284., 276., 348., 14., s), &dark);
            self.text(
                if sticky {
                    "Enter to arrange · Esc to cancel"
                } else if selection.is_some() {
                    "Release to arrange"
                } else {
                    "Release here to cancel"
                },
                12. * s,
                box_rect(32., 289., 272., 312., s),
                &pale,
            )?;
            self.text(
                "Wheel: size   Tab: display   Esc: cancel",
                9. * s,
                box_rect(30., 315., 274., 335., s),
                &muted,
            )?;
            self.target.EndDraw(None, None).map_err(winerr)?;
        }
        if let Some(frame) = frame {
            self.draw_preview(frame, s)?;
        } else {
            self.preview.hide();
        }
        self.ring.present(x, y)
    }
    fn text(
        &self,
        text: &str,
        size: f32,
        bounds: D2D_RECT_F,
        brush: &ID2D1SolidColorBrush,
    ) -> Result<()> {
        unsafe {
            let format = self
                .write
                .CreateTextFormat(
                    w!("Segoe UI"),
                    None,
                    DWRITE_FONT_WEIGHT_MEDIUM,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    size,
                    w!("en-US"),
                )
                .map_err(winerr)?;
            format
                .SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)
                .map_err(winerr)?;
            format
                .SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)
                .map_err(winerr)?;
            self.target.DrawText(
                &text.encode_utf16().collect::<Vec<_>>(),
                &format,
                &bounds,
                brush,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }
        Ok(())
    }
    fn draw_preview(&mut self, frame: Rect, scale: f32) -> Result<()> {
        let padding = 4;
        let width = frame.width + padding * 2;
        let height = frame.height + padding * 2;
        unsafe {
            let s = self.preview.prepare(width, height)?;
            self.target
                .BindDC(
                    s.dc,
                    &RECT {
                        left: 0,
                        top: 0,
                        right: width,
                        bottom: height,
                    },
                )
                .map_err(winerr)?;
            self.target.BeginDraw();
            self.target.Clear(Some(&color(0, 0, 0, 0.)));
            let fill = self
                .target
                .CreateSolidColorBrush(&color(109, 232, 209, 0.14), None)
                .map_err(winerr)?;
            let outline = self
                .target
                .CreateSolidColorBrush(&color(109, 232, 209, 0.95), None)
                .map_err(winerr)?;
            let shape = round(
                5.,
                5.,
                width as f32 - 5.,
                height as f32 - 5.,
                10. * scale,
                1.,
            );
            self.target.FillRoundedRectangle(&shape, &fill);
            self.target
                .DrawRoundedRectangle(&shape, &outline, 2. * scale, None);
            self.target.EndDraw(None, None).map_err(winerr)?;
        }
        self.preview.present(frame.x - padding, frame.y - padding)
    }
}
fn color(r: u8, g: u8, b: u8, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: r as f32 / 255.,
        g: g as f32 / 255.,
        b: b as f32 / 255.,
        a,
    }
}
fn box_rect(l: f32, t: f32, r: f32, b: f32, s: f32) -> D2D_RECT_F {
    D2D_RECT_F {
        left: l * s,
        top: t * s,
        right: r * s,
        bottom: b * s,
    }
}
fn round(l: f32, t: f32, r: f32, b: f32, radius: f32, s: f32) -> D2D1_ROUNDED_RECT {
    D2D1_ROUNDED_RECT {
        rect: box_rect(l, t, r, b, s),
        radiusX: radius * s,
        radiusY: radius * s,
    }
}
