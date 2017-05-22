use super::nuklear_rust::NkContext;

use super::winapi;
use super::kernel32;
use super::user32;

use std::{ptr, mem, str};
use std::os::windows::ffi::OsStrExt;
use std::ffi::OsStr;

use super::Drawer;

pub fn create_env(window_name: &str, width: u16, height: u16) -> (winapi::HWND, winapi::HDC) {
    unsafe {
        let window = create_window(window_name, width as i32, height as i32);
        let hdc = user32::GetDC(window);
        (window, hdc)
    }
}

pub fn process_events(drawer: &mut Drawer, ctx: &mut NkContext) {
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
        let class_name = OsStr::new("NuklearWindowClass")
            .encode_wide()
            .chain(Some(0).into_iter())
            .collect::<Vec<_>>();

        let class = winapi::WNDCLASSEXW {
            cbSize: mem::size_of::<winapi::WNDCLASSEXW>() as winapi::UINT,
            style: winapi::CS_DBLCLKS,
            lpfnWndProc: Some(callback),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: kernel32::GetModuleHandleW(ptr::null()),
            hIcon: user32::LoadIconW(ptr::null_mut(), winapi::IDI_APPLICATION),
            hCursor: user32::LoadCursorW(ptr::null_mut(), winapi::IDC_ARROW),
            hbrBackground: ptr::null_mut(),
            lpszMenuName: ptr::null(),
            lpszClassName: class_name.as_ptr(),
            hIconSm: ptr::null_mut(),
        };
        user32::RegisterClassExW(&class);

        class_name
    }
}

unsafe fn create_window(window_name: &str, width: i32, height: i32) -> winapi::HWND {
    let class_name = register_window_class();

    let mut rect = winapi::RECT {
        left: 0,
        top: 0,
        right: width,
        bottom: height,
    };
    let style = winapi::WS_OVERLAPPEDWINDOW;
    let exstyle = winapi::WS_EX_APPWINDOW;

    user32::AdjustWindowRectEx(&mut rect, style, winapi::FALSE, exstyle);
    let window_name = OsStr::new(window_name)
        .encode_wide()
        .chain(Some(0).into_iter())
        .collect::<Vec<_>>();

    user32::CreateWindowExW(exstyle,
                            class_name.as_ptr(),
                            window_name.as_ptr() as winapi::LPCWSTR,
                            style | winapi::WS_VISIBLE,
                            winapi::CW_USEDEFAULT,
                            winapi::CW_USEDEFAULT,
                            rect.right - rect.left,
                            rect.bottom - rect.top,
                            ptr::null_mut(),
                            ptr::null_mut(),
                            kernel32::GetModuleHandleW(ptr::null()),
                            ptr::null_mut())
}

unsafe extern "system" fn callback(wnd: winapi::HWND, msg: winapi::UINT, wparam: winapi::WPARAM, lparam: winapi::LPARAM) -> winapi::LRESULT {
    match msg {
        winapi::WM_DESTROY => {
            user32::PostQuitMessage(0);
            return 0;
        }
        _ => {
            if EVENTS.is_none() {
                EVENTS = Some(Vec::new());
            }
            EVENTS
                .as_mut()
                .unwrap()
                .push((wnd, msg, wparam, lparam));
        }
    }

    user32::DefWindowProcW(wnd, msg, wparam, lparam)
}

static mut EVENTS: Option<Vec<(winapi::HWND, winapi::UINT, winapi::WPARAM, winapi::LPARAM)>> = None;
