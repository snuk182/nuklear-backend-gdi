use super::*;

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::{mem, ptr, str};

pub fn create_env(window_name: &str, width: u16, height: u16) -> (windef::HWND, windef::HDC) {
    unsafe {
        let window = create_window(window_name, width as i32, height as i32);
        let hdc = winuser::GetDC(window);
        (window, hdc)
    }
}

pub fn process_events(drawer: &mut Drawer, ctx: &mut Context) {
    unsafe {
        if EVENTS.is_none() {
            EVENTS = Some(Vec::new());
        }

        for &mut (wnd, msg, wparam, lparam) in EVENTS.as_mut().unwrap() {
            drawer.handle_event(ctx, wnd, msg, wparam, lparam);
        }

        EVENTS.as_mut().unwrap().clear();
    }
}

fn register_window_class() -> Vec<u16> {
    unsafe {
        let class_name = OsStr::new("NuklearWindowClass").encode_wide().chain(Some(0).into_iter()).collect::<Vec<_>>();

        let class = winuser::WNDCLASSEXW {
            cbSize: mem::size_of::<winuser::WNDCLASSEXW>() as minwindef::UINT,
            style: winuser::CS_DBLCLKS,
            lpfnWndProc: Some(callback),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: libloaderapi::GetModuleHandleW(ptr::null()),
            hIcon: winuser::LoadIconW(ptr::null_mut(), winuser::IDI_APPLICATION),
            hCursor: winuser::LoadCursorW(ptr::null_mut(), winuser::IDC_ARROW),
            hbrBackground: ptr::null_mut(),
            lpszMenuName: ptr::null(),
            lpszClassName: class_name.as_ptr(),
            hIconSm: ptr::null_mut(),
        };
        winuser::RegisterClassExW(&class);

        class_name
    }
}

unsafe fn create_window(window_name: &str, width: i32, height: i32) -> windef::HWND {
    let class_name = register_window_class();

    let mut rect = windef::RECT { left: 0, top: 0, right: width, bottom: height };
    let style = winuser::WS_OVERLAPPEDWINDOW;
    let exstyle = winuser::WS_EX_APPWINDOW;

    winuser::AdjustWindowRectEx(&mut rect, style, minwindef::FALSE, exstyle);
    let window_name = OsStr::new(window_name).encode_wide().chain(Some(0).into_iter()).collect::<Vec<_>>();

    winuser::CreateWindowExW(
        exstyle,
        class_name.as_ptr(),
        window_name.as_ptr() as ntdef::LPCWSTR,
        style | winuser::WS_VISIBLE,
        winuser::CW_USEDEFAULT,
        winuser::CW_USEDEFAULT,
        rect.right - rect.left,
        rect.bottom - rect.top,
        ptr::null_mut(),
        ptr::null_mut(),
        libloaderapi::GetModuleHandleW(ptr::null()),
        ptr::null_mut(),
    )
}

unsafe extern "system" fn callback(wnd: windef::HWND, msg: minwindef::UINT, wparam: minwindef::WPARAM, lparam: minwindef::LPARAM) -> minwindef::LRESULT {
    match msg {
        winuser::WM_DESTROY => {
            winuser::PostQuitMessage(0);
            return 0;
        }
        _ => {
            if EVENTS.is_none() {
                EVENTS = Some(Vec::new());
            }
            EVENTS.as_mut().unwrap().push((wnd, msg, wparam, lparam));
        }
    }

    winuser::DefWindowProcW(wnd, msg, wparam, lparam)
}

static mut EVENTS: Option<Vec<(windef::HWND, minwindef::UINT, minwindef::WPARAM, minwindef::LPARAM)>> = None;
