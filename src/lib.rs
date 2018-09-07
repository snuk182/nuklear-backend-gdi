#![cfg_attr(feature = "cargo-clippy", allow(cast_ptr_alignment))] // API requirement
#![cfg_attr(feature = "cargo-clippy", allow(cast_lossless))] // API requirement
#![cfg_attr(feature = "cargo-clippy", allow(redundant_field_names))] // for conststency
#![cfg_attr(feature = "cargo-clippy", allow(too_many_arguments))] // TODO later
#![cfg_attr(feature = "cargo-clippy", allow(many_single_char_names))] // TODO later

extern crate nuklear;

extern crate winapi;

use winapi::ctypes;
use winapi::ctypes::c_void;
use winapi::shared::basetsd;
use winapi::shared::minwindef;
use winapi::shared::ntdef;
use winapi::shared::windef;
use winapi::um::errhandlingapi;
use winapi::um::libloaderapi;
use winapi::um::stringapiset;
use winapi::um::winbase;
use winapi::um::wingdi;
use winapi::um::winnls;
use winapi::um::winuser;

#[cfg(feature = "piston_image")]
extern crate image;
#[cfg(feature = "own_window")]
mod own_window;

use nuklear::nuklear_sys as nksys;
use nuklear::*;
use std::{ffi, mem, ptr, slice, str};

pub type GdiFontID = usize;

struct GdiFont {
    nk: nksys::nk_user_font,
    height: i32,
    handle: windef::HFONT,
    dc: windef::HDC,
}

impl GdiFont {
    pub unsafe fn new(name: &str, size: i32) -> GdiFont {
        let mut metric = wingdi::TEXTMETRICW {
            tmHeight: 0,
            tmAscent: 0,
            tmDescent: 0,
            tmInternalLeading: 0,
            tmExternalLeading: 0,
            tmAveCharWidth: 0,
            tmMaxCharWidth: 0,
            tmWeight: 0,
            tmOverhang: 0,
            tmDigitizedAspectX: 0,
            tmDigitizedAspectY: 0,
            tmFirstChar: 0,
            tmLastChar: 0,
            tmDefaultChar: 0,
            tmBreakChar: 0,
            tmItalic: 0,
            tmUnderlined: 0,
            tmStruckOut: 0,
            tmPitchAndFamily: 0,
            tmCharSet: 0,
        };
        let handle = wingdi::CreateFontA(
            size,
            0,
            0,
            0,
            wingdi::FW_NORMAL,
            minwindef::FALSE as u32,
            minwindef::FALSE as u32,
            minwindef::FALSE as u32,
            wingdi::DEFAULT_CHARSET,
            wingdi::OUT_DEFAULT_PRECIS,
            wingdi::CLIP_DEFAULT_PRECIS,
            wingdi::CLEARTYPE_QUALITY,
            wingdi::DEFAULT_PITCH | wingdi::FF_DONTCARE,
            name.as_ptr() as *const i8,
        );
        let dc = wingdi::CreateCompatibleDC(ptr::null_mut());

        wingdi::SelectObject(dc, handle as *mut c_void);
        wingdi::GetTextMetricsW(dc, &mut metric);

        GdiFont {
            nk: mem::uninitialized(),
            height: metric.tmHeight,
            handle: handle as windef::HFONT,
            dc: dc,
        }
    }
}

impl Drop for GdiFont {
    fn drop(&mut self) {
        unsafe {
            wingdi::DeleteObject(self.handle as *mut c_void);
            wingdi::DeleteDC(self.dc);
        }
    }
}

pub struct Drawer {
    bitmap: windef::HBITMAP,
    window_dc: windef::HDC,
    memory_dc: windef::HDC,
    width: i32,
    height: i32,
    fonts: Vec<GdiFont>,

    window: Option<windef::HWND>,
}

impl Drawer {
    pub fn new(window_dc: windef::HDC, width: u16, height: u16, window: Option<windef::HWND>) -> Drawer {
        unsafe {
            let drawer = Drawer {
                bitmap: wingdi::CreateCompatibleBitmap(window_dc, width as i32, height as i32),
                window_dc: window_dc,
                memory_dc: wingdi::CreateCompatibleDC(window_dc),
                width: width as i32,
                height: height as i32,
                fonts: Vec::new(),

                window: window,
            };
            wingdi::SelectObject(drawer.memory_dc, drawer.bitmap as *mut c_void);

            drawer
        }
    }

    pub fn install_statics(&self, context: &mut Context) {
        unsafe {
            let context: &mut nksys::nk_context = &mut *(context as *mut _ as *mut nksys::nk_context);
            context.clip.copy = Some(nk_gdi_clipbard_copy);
            context.clip.paste = Some(nk_gdi_clipbard_paste);
        }
    }

    pub fn window(&self) -> Option<windef::HWND> {
        self.window
    }

    pub fn process_events(&mut self, ctx: &mut Context) -> bool {
        unsafe {
            let mut msg: winuser::MSG = mem::zeroed();
            ctx.input_begin();

            if winuser::GetMessageW(&mut msg, ptr::null_mut(), 0, 0) <= 0 {
                return false;
            } else {
                winuser::TranslateMessage(&msg);
                winuser::DispatchMessageW(&msg);
            }

            #[cfg(feature = "own_window")]
            own_window::process_events(self, ctx);

            ctx.input_end();
        }
        true
    }

    pub fn new_font(&mut self, name: &str, size: u16) -> GdiFontID {
        self.fonts.push(unsafe { GdiFont::new(name, size as i32) });

        let index = self.fonts.len() - 1;
        let gdifont = &mut self.fonts[index];

        unsafe {
            ptr::write(
                &mut gdifont.nk,
                nksys::nk_user_font {
                    userdata: nksys::nk_handle_ptr(gdifont as *mut _ as *mut ::std::os::raw::c_void),
                    height: gdifont.height as f32,
                    width: None,
                    query: None,
                    texture: nksys::nk_handle::default(),
                },
            );

            gdifont.nk.height = gdifont.height as f32;
            gdifont.nk.width = Some(nk_gdifont_get_text_width);
        }

        index as GdiFontID
    }

    pub fn font_by_id(&self, id: GdiFontID) -> Option<&UserFont> {
        if self.fonts.len() <= id {
            return None;
        }

        Some(unsafe { &*(&self.fonts[id].nk as *const _ as *const UserFont) })
    }

    #[cfg(feature = "piston_image")]
    pub fn add_image(&mut self, img: &image::DynamicImage) -> Handle {
        use image::GenericImage;

        let (w, h) = img.dimensions();

        let hbmp: windef::HBITMAP;

        let bminfo = wingdi::BITMAPINFO {
            bmiHeader: wingdi::BITMAPINFOHEADER {
                biSize: mem::size_of::<wingdi::BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: h as i32,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: wingdi::BI_RGB,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: unsafe { mem::zeroed() },
        };

        unsafe {
            let mut pv_image_bits = ptr::null_mut();
            let hdc_screen = winuser::GetDC(ptr::null_mut());
            hbmp = wingdi::CreateDIBSection(hdc_screen, &bminfo, wingdi::DIB_RGB_COLORS, &mut pv_image_bits, ptr::null_mut(), 0);
            winuser::ReleaseDC(ptr::null_mut(), hdc_screen);
            if hbmp.is_null() {
                panic!("Out of memory!");
            }

            let mut img = img.flipv().to_rgba();
            for x in 0..w {
                for y in 0..h {
                    let p = img.get_pixel_mut(x, y);
                    let r = p[0];
                    let b = p[2];
                    p[0] = b;
                    p[2] = r;
                }
            }
            ptr::copy(img.into_raw().as_ptr(), pv_image_bits as *mut u8, (w * h * 4) as usize);
            /*{
		        // couldn't extract image; delete HBITMAP
		        wingdi::DeleteObject(hbmp as *mut c_void);
		        hbmp = ptr::null_mut();
		        return;
		    }*/
            //TODO
            Handle::from_ptr(hbmp as *mut ::std::os::raw::c_void)
        }
    }

    pub fn handle_event(&mut self, ctx: &mut Context, wnd: windef::HWND, msg: minwindef::UINT, wparam: minwindef::WPARAM, lparam: minwindef::LPARAM) -> bool {
        match msg {
            winuser::WM_SIZE => {
                let width = lparam as u16;
                let height = (lparam >> 16) as u16;
                if width as i32 != self.width || height as i32 != self.height {
                    unsafe {
                        wingdi::DeleteObject(self.bitmap as *mut c_void);
                        self.bitmap = wingdi::CreateCompatibleBitmap(self.window_dc, width as i32, height as i32);
                        self.width = width as i32;
                        self.height = height as i32;
                        wingdi::SelectObject(self.memory_dc, self.bitmap as *mut c_void);
                    }
                }
            }
            winuser::WM_PAINT => {
                unsafe {
                    let mut paint: winuser::PAINTSTRUCT = mem::zeroed();
                    let dc = winuser::BeginPaint(wnd, &mut paint);
                    self.blit(dc);
                    winuser::EndPaint(wnd, &paint);
                }
                return true;
            }
            winuser::WM_KEYDOWN | winuser::WM_KEYUP | winuser::WM_SYSKEYDOWN | winuser::WM_SYSKEYUP => {
                let down = ((lparam >> 31) & 1) == 0;
                let ctrl = unsafe { (winuser::GetKeyState(winuser::VK_CONTROL) & (1 << 15)) != 0 };

                match wparam as i32 {
                    winuser::VK_SHIFT | winuser::VK_LSHIFT | winuser::VK_RSHIFT => {
                        ctx.input_key(Key::NK_KEY_SHIFT, down);
                        return true;
                    }
                    winuser::VK_DELETE => {
                        ctx.input_key(Key::NK_KEY_DEL, down);
                        return true;
                    }
                    winuser::VK_RETURN => {
                        ctx.input_key(Key::NK_KEY_ENTER, down);
                        return true;
                    }
                    winuser::VK_TAB => {
                        ctx.input_key(Key::NK_KEY_TAB, down);
                        return true;
                    }
                    winuser::VK_LEFT => {
                        if ctrl {
                            ctx.input_key(Key::NK_KEY_TEXT_WORD_LEFT, down);
                        } else {
                            ctx.input_key(Key::NK_KEY_LEFT, down);
                        }
                        return true;
                    }
                    winuser::VK_RIGHT => {
                        if ctrl {
                            ctx.input_key(Key::NK_KEY_TEXT_WORD_RIGHT, down);
                        } else {
                            ctx.input_key(Key::NK_KEY_RIGHT, down);
                        }
                        return true;
                    }
                    winuser::VK_BACK => {
                        ctx.input_key(Key::NK_KEY_BACKSPACE, down);
                        return true;
                    }
                    winuser::VK_HOME => {
                        ctx.input_key(Key::NK_KEY_TEXT_START, down);
                        ctx.input_key(Key::NK_KEY_SCROLL_START, down);
                        return true;
                    }
                    winuser::VK_END => {
                        ctx.input_key(Key::NK_KEY_TEXT_END, down);
                        ctx.input_key(Key::NK_KEY_SCROLL_END, down);
                        return true;
                    }
                    winuser::VK_NEXT => {
                        ctx.input_key(Key::NK_KEY_SCROLL_DOWN, down);
                        return true;
                    }
                    winuser::VK_PRIOR => {
                        ctx.input_key(Key::NK_KEY_SCROLL_UP, down);
                        return true;
                    }
                    _ => {}
                }
                match wparam as u8 as char {
                    'C' => {
                        if ctrl {
                            ctx.input_key(Key::NK_KEY_COPY, down);
                            return true;
                        }
                    }
                    'V' => {
                        if ctrl {
                            ctx.input_key(Key::NK_KEY_PASTE, down);
                            return true;
                        }
                    }
                    'X' => {
                        if ctrl {
                            ctx.input_key(Key::NK_KEY_CUT, down);
                            return true;
                        }
                    }
                    'Z' => {
                        if ctrl {
                            ctx.input_key(Key::NK_KEY_TEXT_UNDO, down);
                            return true;
                        }
                    }
                    'R' => {
                        if ctrl {
                            ctx.input_key(Key::NK_KEY_TEXT_REDO, down);
                            return true;
                        }
                    }
                    _ => {}
                }
            }
            winuser::WM_CHAR => {
                if wparam >= 32 {
                    unsafe {
                        ctx.input_unicode(::std::char::from_u32_unchecked(wparam as u32));
                    }
                    return true;
                }
            }
            winuser::WM_LBUTTONDOWN => {
                ctx.input_button(Button::NK_BUTTON_LEFT, lparam as u16 as i32, (lparam >> 16) as u16 as i32, true);
                unsafe {
                    winuser::SetCapture(wnd);
                }
                return true;
            }
            winuser::WM_LBUTTONUP => {
                ctx.input_button(Button::NK_BUTTON_LEFT, lparam as u16 as i32, (lparam >> 16) as u16 as i32, false);
                unsafe {
                    winuser::ReleaseCapture();
                }
                return true;
            }
            winuser::WM_RBUTTONDOWN => {
                ctx.input_button(Button::NK_BUTTON_RIGHT, lparam as u16 as i32, (lparam >> 16) as u16 as i32, true);
                unsafe {
                    winuser::SetCapture(wnd);
                }
                return true;
            }
            winuser::WM_RBUTTONUP => {
                ctx.input_button(Button::NK_BUTTON_RIGHT, lparam as u16 as i32, (lparam >> 16) as u16 as i32, false);
                unsafe {
                    winuser::ReleaseCapture();
                }
                return true;
            }
            winuser::WM_MBUTTONDOWN => {
                ctx.input_button(Button::NK_BUTTON_MIDDLE, lparam as u16 as i32, (lparam >> 16) as u16 as i32, true);
                unsafe {
                    winuser::SetCapture(wnd);
                }
                return true;
            }
            winuser::WM_MBUTTONUP => {
                ctx.input_button(Button::NK_BUTTON_MIDDLE, lparam as u16 as i32, (lparam >> 16) as u16 as i32, false);
                unsafe {
                    winuser::ReleaseCapture();
                }
                return true;
            }
            winuser::WM_MOUSEWHEEL => {
                ctx.input_scroll(((wparam >> 16) as u16) as f32 / winuser::WHEEL_DELTA as f32);
                return true;
            }
            winuser::WM_MOUSEMOVE => {
                ctx.input_motion(lparam as u16 as i32, (lparam >> 16) as u16 as i32);
                return true;
            }
            _ => {}
        }
        false
    }

    pub fn render(&self, ctx: &mut Context, clear: Color) {
        unsafe {
            let memory_dc = self.memory_dc;
            wingdi::SelectObject(memory_dc, wingdi::GetStockObject(wingdi::DC_PEN as i32));
            wingdi::SelectObject(memory_dc, wingdi::GetStockObject(wingdi::DC_BRUSH as i32));
            self.clear_dc(memory_dc, clear);

            for cmd in ctx.command_iterator() {
                match cmd.get_type() {
                    CommandType::NK_COMMAND_ARC_FILLED => {
                        let a: &CommandArcFilled = cmd.as_ref();
                        nk_gdi_fill_arc(memory_dc, a.cx() as i32, a.cy() as i32, a.r() as u32, a.a()[0], a.a()[1], a.color());
                    }
                    CommandType::NK_COMMAND_ARC => {
                        let a: &CommandArc = cmd.as_ref();
                        nk_gdi_stroke_arc(memory_dc, a.cx() as i32, a.cy() as i32, a.r() as u32, a.a()[0], a.a()[1], a.line_thickness() as i32, a.color());
                    }
                    CommandType::NK_COMMAND_SCISSOR => {
                        let s: &CommandScissor = cmd.as_ref();
                        nk_gdi_scissor(memory_dc, s.x() as f32, s.y() as f32, s.w() as f32, s.h() as f32);
                    }
                    CommandType::NK_COMMAND_LINE => {
                        let l: &CommandLine = cmd.as_ref();
                        nk_gdi_stroke_line(memory_dc, l.begin().x as i32, l.begin().y as i32, l.end().x as i32, l.end().y as i32, l.line_thickness() as i32, l.color());
                    }
                    CommandType::NK_COMMAND_RECT => {
                        let r: &CommandRect = cmd.as_ref();
                        nk_gdi_stroke_rect(memory_dc, r.x() as i32, r.y() as i32, r.w() as i32, r.h() as i32, r.rounding() as u16 as i32, r.line_thickness() as i32, r.color());
                    }
                    CommandType::NK_COMMAND_RECT_FILLED => {
                        let r: &CommandRectFilled = cmd.as_ref();
                        nk_gdi_fill_rect(memory_dc, r.x() as i32, r.y() as i32, r.w() as i32, r.h() as i32, r.rounding() as u16 as i32, r.color());
                    }
                    CommandType::NK_COMMAND_CIRCLE => {
                        let c: &CommandCircle = cmd.as_ref();
                        nk_gdi_stroke_circle(memory_dc, c.x() as i32, c.y() as i32, c.w() as i32, c.h() as i32, c.line_thickness() as i32, c.color());
                    }
                    CommandType::NK_COMMAND_CIRCLE_FILLED => {
                        let c: &CommandCircleFilled = cmd.as_ref();
                        nk_gdi_fill_circle(memory_dc, c.x() as i32, c.y() as i32, c.w() as i32, c.h() as i32, c.color());
                    }
                    CommandType::NK_COMMAND_TRIANGLE => {
                        let t: &CommandTriangle = cmd.as_ref();
                        nk_gdi_stroke_triangle(memory_dc, t.a().x as i32, t.a().y as i32, t.b().x as i32, t.b().y as i32, t.c().x as i32, t.c().y as i32, t.line_thickness() as i32, t.color());
                    }
                    CommandType::NK_COMMAND_TRIANGLE_FILLED => {
                        let t: &CommandTriangleFilled = cmd.as_ref();
                        nk_gdi_fill_triangle(memory_dc, t.a().x as i32, t.a().y as i32, t.b().x as i32, t.b().y as i32, t.c().x as i32, t.c().y as i32, t.color());
                    }
                    CommandType::NK_COMMAND_POLYGON => {
                        let p: &CommandPolygon = cmd.as_ref();
                        nk_gdi_stroke_polygon(memory_dc, p.points().as_ptr(), p.points().len() as usize, p.line_thickness() as i32, p.color());
                    }
                    CommandType::NK_COMMAND_POLYGON_FILLED => {
                        let p: &CommandPolygonFilled = cmd.as_ref();
                        nk_gdi_fill_polygon(memory_dc, p.points().as_ptr(), p.points().len() as usize, p.color());
                    }
                    CommandType::NK_COMMAND_POLYLINE => {
                        let p: &CommandPolyline = cmd.as_ref();
                        nk_gdi_stroke_polyline(memory_dc, p.points().as_ptr(), p.points().len() as usize, p.line_thickness() as i32, p.color());
                    }
                    CommandType::NK_COMMAND_TEXT => {
                        let t: &CommandText = cmd.as_ref();
                        nk_gdi_draw_text(
                            memory_dc,
                            t.x() as i32,
                            t.y() as i32,
                            t.w() as i32,
                            t.h() as i32,
                            t.chars().as_ptr() as *const i8,
                            t.chars().len() as i32,
                            (t.font()).userdata_ptr().ptr().unwrap() as *const GdiFont,
                            t.background(),
                            t.foreground(),
                        );
                    }
                    CommandType::NK_COMMAND_CURVE => {
                        let q: &CommandCurve = cmd.as_ref();
                        nk_gdi_stroke_curve(memory_dc, q.begin(), q.ctrl()[0], q.ctrl()[1], q.end(), q.line_thickness() as i32, q.color());
                    }
                    CommandType::NK_COMMAND_IMAGE => {
                        let i: &CommandImage = cmd.as_ref();
                        nk_gdi_draw_image(memory_dc, i.x() as i32, i.y() as i32, i.w() as i32, i.h() as i32, i.img(), i.col());
                    }
                    _ => {}
                }
            }
            self.blit(self.window_dc);
            ctx.clear();
        }
    }

    unsafe fn clear_dc(&self, dc: windef::HDC, col: Color) {
        let color = convert_color(col);
        let rect = windef::RECT {
            left: 0,
            top: 0,
            right: self.width,
            bottom: self.height,
        };
        wingdi::SetBkColor(dc, color);

        wingdi::ExtTextOutW(dc, 0, 0, wingdi::ETO_OPAQUE, &rect, ptr::null_mut(), 0, ptr::null_mut());
    }

    unsafe fn blit(&self, dc: windef::HDC) {
        wingdi::BitBlt(dc, 0, 0, self.width, self.height, self.memory_dc, 0, 0, wingdi::SRCCOPY);
    }
}

impl Drop for Drawer {
    fn drop(&mut self) {
        unsafe {
            wingdi::DeleteObject(self.memory_dc as *mut c_void);
            wingdi::DeleteObject(self.bitmap as *mut c_void);
        }
    }
}

fn convert_color(c: Color) -> windef::COLORREF {
    c.r as u32 | ((c.g as u32) << 8) | ((c.b as u32) << 16)
}

unsafe fn nk_gdi_scissor(dc: windef::HDC, x: f32, y: f32, w: f32, h: f32) {
    wingdi::SelectClipRgn(dc, ptr::null_mut());
    wingdi::IntersectClipRect(dc, x as i32, y as i32, (x + w + 1.0) as i32, (y + h + 1.0) as i32);
}

unsafe fn nk_gdi_stroke_line(dc: windef::HDC, x0: i32, y0: i32, x1: i32, y1: i32, line_thickness: i32, col: Color) {
    let color = convert_color(col);

    let mut pen = ptr::null_mut();
    if line_thickness == 1 {
        wingdi::SetDCPenColor(dc, color);
    } else {
        pen = wingdi::CreatePen(wingdi::PS_SOLID as i32, line_thickness, color);
        wingdi::SelectObject(dc, pen as *mut c_void);
    }

    wingdi::MoveToEx(dc, x0, y0, ptr::null_mut());
    wingdi::LineTo(dc, x1, y1);

    if !pen.is_null() {
        wingdi::SelectObject(dc, wingdi::GetStockObject(wingdi::DC_PEN as i32));
        wingdi::DeleteObject(pen as *mut c_void);
    }
}

unsafe fn nk_gdi_stroke_rect(dc: windef::HDC, x: i32, y: i32, w: i32, h: i32, r: i32, line_thickness: i32, col: Color) {
    let color = convert_color(col);

    let mut pen = ptr::null_mut();
    if line_thickness == 1 {
        wingdi::SetDCPenColor(dc, color);
    } else {
        pen = wingdi::CreatePen(wingdi::PS_SOLID as i32, line_thickness, color);
        wingdi::SelectObject(dc, pen as *mut c_void);
    }

    if r == 0 {
        wingdi::Rectangle(dc, x, y, x + w, y + h);
    } else {
        wingdi::RoundRect(dc, x, y, x + w, y + h, r, r);
    }

    if !pen.is_null() {
        wingdi::SelectObject(dc, wingdi::GetStockObject(wingdi::DC_PEN as i32));
        wingdi::DeleteObject(pen as *mut c_void);
    }
}

unsafe fn nk_gdi_fill_rect(dc: windef::HDC, x: i32, y: i32, w: i32, h: i32, r: i32, col: Color) {
    let color = convert_color(col);

    if r == 0 {
        let rect = windef::RECT { left: x, top: y, right: x + w, bottom: y + h };
        wingdi::SetBkColor(dc, color);
        wingdi::ExtTextOutW(dc, 0, 0, wingdi::ETO_OPAQUE, &rect, ptr::null_mut(), 0, ptr::null_mut());
    } else {
        wingdi::SetDCPenColor(dc, color);
        wingdi::SetDCBrushColor(dc, color);
        wingdi::RoundRect(dc, x, y, x + w, y + h, r, r);
    }
    wingdi::SetDCBrushColor(dc, color);
}

unsafe fn nk_gdi_fill_triangle(dc: windef::HDC, x0: i32, y0: i32, x1: i32, y1: i32, x2: i32, y2: i32, col: Color) {
    let color = convert_color(col);
    let points = [windef::POINT { x: x0, y: y0 }, windef::POINT { x: x1, y: y1 }, windef::POINT { x: x2, y: y2 }];

    wingdi::SetDCPenColor(dc, color);
    wingdi::SetDCBrushColor(dc, color);
    wingdi::Polygon(dc, &points[0] as *const windef::POINT, points.len() as i32);
}

unsafe fn nk_gdi_stroke_triangle(dc: windef::HDC, x0: i32, y0: i32, x1: i32, y1: i32, x2: i32, y2: i32, line_thickness: i32, col: Color) {
    let color = convert_color(col);
    let points = [windef::POINT { x: x0, y: y0 }, windef::POINT { x: x1, y: y1 }, windef::POINT { x: x2, y: y2 }, windef::POINT { x: x0, y: y0 }];

    let mut pen = ptr::null_mut();
    if line_thickness == 1 {
        wingdi::SetDCPenColor(dc, color);
    } else {
        pen = wingdi::CreatePen(wingdi::PS_SOLID as i32, line_thickness, color);
        wingdi::SelectObject(dc, pen as *mut c_void);
    }

    wingdi::Polyline(dc, &points[0] as *const windef::POINT, points.len() as i32);

    if !pen.is_null() {
        wingdi::SelectObject(dc, wingdi::GetStockObject(wingdi::DC_PEN as i32));
        wingdi::DeleteObject(pen as *mut c_void);
    }
}

unsafe fn nk_gdi_fill_polygon(dc: windef::HDC, pnts: *const Vec2i, count: usize, col: Color) {
    if count < 1 {
        return;
    }

    let mut points = [windef::POINT { x: 0, y: 0 }; 64];
    let color = convert_color(col);
    wingdi::SetDCBrushColor(dc, color);
    wingdi::SetDCPenColor(dc, color);
    let pnts = slice::from_raw_parts(pnts, count);
    for (i, pnt) in pnts.iter().enumerate() {
        points[i].x = pnt.x as i32;
        points[i].y = pnt.y as i32;
    }
    wingdi::Polygon(dc, &points[0], pnts.len() as i32);
}

unsafe fn nk_gdi_stroke_polygon(dc: windef::HDC, pnts: *const Vec2i, count: usize, line_thickness: i32, col: Color) {
    let color = convert_color(col);
    let mut pen = ptr::null_mut();
    if line_thickness == 1 {
        wingdi::SetDCPenColor(dc, color);
    } else {
        pen = wingdi::CreatePen(wingdi::PS_SOLID as i32, line_thickness, color);
        wingdi::SelectObject(dc, pen as *mut c_void);
    }

    if count > 0 {
        let pnts = slice::from_raw_parts(pnts, count);
        wingdi::MoveToEx(dc, pnts[0].x as i32, pnts[0].y as i32, ptr::null_mut());
        for pnt in pnts.iter().skip(1) {
            wingdi::LineTo(dc, pnt.x as i32, pnt.y as i32);
        }
        wingdi::LineTo(dc, pnts[0].x as i32, pnts[0].y as i32);
    }

    if !pen.is_null() {
        wingdi::SelectObject(dc, wingdi::GetStockObject(wingdi::DC_PEN as i32));
        wingdi::DeleteObject(pen as *mut c_void);
    }
}

unsafe fn nk_gdi_stroke_polyline(dc: windef::HDC, pnts: *const Vec2i, count: usize, line_thickness: i32, col: Color) {
    let color = convert_color(col);
    let mut pen = ptr::null_mut();
    if line_thickness == 1 {
        wingdi::SetDCPenColor(dc, color);
    } else {
        pen = wingdi::CreatePen(wingdi::PS_SOLID as i32, line_thickness, color);
        wingdi::SelectObject(dc, pen as *mut c_void);
    }

    if count > 0 {
        let pnts = slice::from_raw_parts(pnts, count);
        wingdi::MoveToEx(dc, pnts[0].x as i32, pnts[0].y as i32, ptr::null_mut());
        for pnt in pnts.iter().skip(1) {
            wingdi::LineTo(dc, pnt.x as i32, pnt.y as i32);
        }
    }

    if !pen.is_null() {
        wingdi::SelectObject(dc, wingdi::GetStockObject(wingdi::DC_PEN as i32));
        wingdi::DeleteObject(pen as *mut c_void);
    }
}

unsafe fn nk_gdi_fill_arc(dc: windef::HDC, cx: i32, cy: i32, r: u32, a1: f32, a2: f32, color: Color) {
    let color = convert_color(color);
    wingdi::SetDCBrushColor(dc, color);
    wingdi::SetDCPenColor(dc, color);
    wingdi::AngleArc(dc, cx, cy, r, a1, a2);
}

unsafe fn nk_gdi_stroke_arc(dc: windef::HDC, cx: i32, cy: i32, r: u32, a1: f32, a2: f32, line_thickness: i32, col: Color) {
    let color = convert_color(col);
    let mut pen = ptr::null_mut();
    if line_thickness == 1 {
        wingdi::SetDCPenColor(dc, color);
    } else {
        pen = wingdi::CreatePen(wingdi::PS_SOLID as i32, line_thickness, color);
        wingdi::SelectObject(dc, pen as *mut c_void);
    }

    wingdi::AngleArc(dc, cx, cy, r, a1, a2);

    if !pen.is_null() {
        wingdi::SelectObject(dc, wingdi::GetStockObject(wingdi::DC_PEN as i32));
        wingdi::DeleteObject(pen as *mut c_void);
    }
}

unsafe fn nk_gdi_fill_circle(dc: windef::HDC, x: i32, y: i32, w: i32, h: i32, col: Color) {
    let color = convert_color(col);
    wingdi::SetDCBrushColor(dc, color);
    wingdi::SetDCPenColor(dc, color);
    wingdi::Ellipse(dc, x, y, x + w, y + h);
}

unsafe fn nk_gdi_stroke_circle(dc: windef::HDC, x: i32, y: i32, w: i32, h: i32, line_thickness: i32, col: Color) {
    let color = convert_color(col);
    let mut pen = ptr::null_mut();
    if line_thickness == 1 {
        wingdi::SetDCPenColor(dc, color);
    } else {
        pen = wingdi::CreatePen(wingdi::PS_SOLID as i32, line_thickness, color);
        wingdi::SelectObject(dc, pen as *mut c_void);
    }

    wingdi::Ellipse(dc, x, y, x + w, y + h);

    if !pen.is_null() {
        wingdi::SelectObject(dc, wingdi::GetStockObject(wingdi::DC_PEN as i32));
        wingdi::DeleteObject(pen as *mut c_void);
    }
}

unsafe fn nk_gdi_stroke_curve(dc: windef::HDC, p1: Vec2i, p2: Vec2i, p3: Vec2i, p4: Vec2i, line_thickness: i32, col: Color) {
    let color = convert_color(col);
    let p = [
        windef::POINT { x: p1.x as i32, y: p1.y as i32 },
        windef::POINT { x: p2.x as i32, y: p2.y as i32 },
        windef::POINT { x: p3.x as i32, y: p3.y as i32 },
        windef::POINT { x: p4.x as i32, y: p4.y as i32 },
    ];

    let mut pen = ptr::null_mut();
    if line_thickness == 1 {
        wingdi::SetDCPenColor(dc, color);
    } else {
        pen = wingdi::CreatePen(wingdi::PS_SOLID as i32, line_thickness, color);
        wingdi::SelectObject(dc, pen as *mut c_void);
    }

    wingdi::PolyBezier(dc, &p[0], p.len() as u32);

    if !pen.is_null() {
        wingdi::SelectObject(dc, wingdi::GetStockObject(wingdi::DC_PEN as i32));
        wingdi::DeleteObject(pen as *mut c_void);
    }
}

unsafe fn nk_gdi_draw_image(dc: windef::HDC, x: i32, y: i32, w: i32, h: i32, mut img: Image, _: Color) {
    let mut bitmap: wingdi::BITMAP = mem::zeroed();
    let hdc1 = wingdi::CreateCompatibleDC(ptr::null_mut());
    let h_bitmap = img.ptr() as *mut _ as *mut ctypes::c_void;

    wingdi::GetObjectW(h_bitmap, mem::size_of_val(&bitmap) as i32, &mut bitmap as *mut _ as *mut ctypes::c_void);
    log_error();
    wingdi::SelectObject(hdc1, h_bitmap);
    log_error();

    let blendfunc = wingdi::BLENDFUNCTION {
        BlendOp: wingdi::AC_SRC_OVER,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: wingdi::AC_SRC_ALPHA,
    };

    wingdi::GdiAlphaBlend(dc, x, y, w, h, hdc1, 0, 0, bitmap.bmWidth, bitmap.bmHeight, blendfunc);
    log_error();
    wingdi::DeleteDC(hdc1);
}

unsafe fn nk_gdi_draw_text(dc: windef::HDC, x: i32, y: i32, _: i32, _: i32, text: *const i8, text_len: i32, font: *const GdiFont, cbg: Color, cfg: Color) {
    let wsize = stringapiset::MultiByteToWideChar(winnls::CP_UTF8, 0, text, text_len, ptr::null_mut(), 0);
    let mut wstr = vec![0u16; wsize as usize * mem::size_of::<ctypes::wchar_t>()];
    stringapiset::MultiByteToWideChar(winnls::CP_UTF8, 0, text, text_len, wstr.as_mut_ptr(), wsize);

    wingdi::SetBkColor(dc, convert_color(cbg));
    wingdi::SetTextColor(dc, convert_color(cfg));

    wingdi::SelectObject(dc, (*font).handle as *mut c_void);
    wingdi::ExtTextOutW(dc, x, y, wingdi::ETO_OPAQUE, ptr::null_mut(), wstr.as_mut_ptr(), wsize as u32, ptr::null_mut());
    wingdi::SetDCBrushColor(dc, convert_color(cbg));
}

unsafe extern "C" fn nk_gdifont_get_text_width(handle: nksys::nk_handle, _: f32, text: *const i8, len: i32) -> f32 {
    let font = *handle.ptr.as_ref() as *const GdiFont;
    if font.is_null() || text.is_null() {
        return 0.0;
    }

    let mut size = windef::SIZE { cx: 0, cy: 0 };
    let wsize = stringapiset::MultiByteToWideChar(winnls::CP_UTF8, 0, text, len, ptr::null_mut(), 0);
    let mut wstr: Vec<ctypes::wchar_t> = vec![0; wsize as usize];
    stringapiset::MultiByteToWideChar(winnls::CP_UTF8, 0, text, len, wstr.as_mut_slice() as *mut _ as *mut ctypes::wchar_t, wsize);

    if wingdi::GetTextExtentPoint32W((*font).dc, wstr.as_slice() as *const _ as *const ctypes::wchar_t, wsize, &mut size) > 0 {
        size.cx as f32
    } else {
        -1.0
    }
}

unsafe extern "C" fn nk_gdi_clipbard_paste(_: nksys::nk_handle, edit: *mut nksys::nk_text_edit) {
    if winuser::IsClipboardFormatAvailable(winuser::CF_UNICODETEXT) > 0 && winuser::OpenClipboard(ptr::null_mut()) > 0 {
        let clip = winuser::GetClipboardData(winuser::CF_UNICODETEXT);
        if !clip.is_null() {
            let size = winbase::GlobalSize(clip) - 1;
            if size > 0 {
                let wstr = winbase::GlobalLock(clip);
                if !wstr.is_null() {
                    let size = (size / mem::size_of::<ctypes::wchar_t>() as basetsd::SIZE_T) as i32;
                    let utf8size = stringapiset::WideCharToMultiByte(winnls::CP_UTF8, 0, wstr as *const u16, size, ptr::null_mut(), 0, ptr::null_mut(), ptr::null_mut());
                    if utf8size > 0 {
                        let mut utf8: Vec<u8> = vec![0; utf8size as usize];
                        stringapiset::WideCharToMultiByte(winnls::CP_UTF8, 0, wstr as *const u16, size, utf8.as_mut_ptr() as *mut i8, utf8size, ptr::null_mut(), ptr::null_mut());
                        let edit = &mut *(edit as *mut _ as *mut TextEdit);
                        edit.paste(str::from_utf8_unchecked(utf8.as_slice()));
                    }
                    winbase::GlobalUnlock(clip);
                }
            }
        }
        winuser::CloseClipboard();
    }
}

unsafe extern "C" fn nk_gdi_clipbard_copy(_: nksys::nk_handle, text: *const i8, _: i32) {
    if winuser::OpenClipboard(ptr::null_mut()) > 0 {
        let str_size = ffi::CStr::from_ptr(text).to_bytes().len() as i32;
        let wsize = stringapiset::MultiByteToWideChar(winnls::CP_UTF8, 0, text, str_size, ptr::null_mut(), 0);
        if wsize > 0 {
            let mem = winbase::GlobalAlloc(2, ((wsize + 1) as usize * mem::size_of::<ctypes::wchar_t>()) as basetsd::SIZE_T); // 2 = GMEM_MOVEABLE
            if !mem.is_null() {
                let wstr = winbase::GlobalLock(mem);
                if !wstr.is_null() {
                    stringapiset::MultiByteToWideChar(winnls::CP_UTF8, 0, text, str_size, wstr as *mut u16, wsize);
                    *(wstr.offset((wsize * mem::size_of::<u16>() as i32) as isize) as *mut u8) = 0;
                    winbase::GlobalUnlock(mem);

                    winuser::SetClipboardData(winuser::CF_UNICODETEXT, mem);
                }
            }
        }
        winuser::CloseClipboard();
    }
}

#[cfg(feature = "own_window")]
pub fn bundle(window_name: &str, width: u16, height: u16, font_name: &str, font_size: u16, allocator: &mut Allocator) -> (Drawer, Context, GdiFontID) {
    let (hwnd, hdc) = own_window::create_env(window_name, width, height);

    let mut drawer = Drawer::new(hdc, width, height, Some(hwnd));

    let font_id = drawer.new_font(font_name, font_size);
    let mut context = {
        let font = drawer.font_by_id(font_id).unwrap();
        Context::new(allocator, &font)
    };
    drawer.install_statics(&mut context);

    (drawer, context, font_id as GdiFontID)
}

pub unsafe fn log_error() {
    let error = errhandlingapi::GetLastError();
    if error == 0 {
        return;
    }

    let mut string = vec![0u16; 127];
    winbase::FormatMessageW(
        winbase::FORMAT_MESSAGE_FROM_SYSTEM | winbase::FORMAT_MESSAGE_IGNORE_INSERTS,
        ptr::null_mut(),
        error,
        ntdef::LANG_SYSTEM_DEFAULT as u32,
        string.as_mut_ptr(),
        string.len() as u32,
        ptr::null_mut(),
    );

    println!("Last error #{}: {}", error, std::string::String::from_utf16_lossy(&string));
}
