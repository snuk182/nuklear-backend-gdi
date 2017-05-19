use super::nuklear_rust;

use super::winapi;
use super::kernel32;
use super::user32;

use std::{ptr, mem, str};
use std::os::windows::ffi::OsStrExt;
use std::ffi::OsStr;
use std::rc::Rc;
use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use super::Drawer;

pub fn create_env(window_name: &str, width: u16, height: u16) -> (winapi::HWND, winapi::HDC) {
	unsafe { 
		let window = create_window(window_name, width as i32, height as i32);
		let hdc = user32::GetDC(window);
	    (window, hdc)
	}
}

pub fn set_drawer_sync(drawer: Option<Rc<RefCell<Drawer>>>) {
	unsafe {
		DRAWER_SYNC = drawer;
	}
}
pub fn set_context_sync(context: Option<Rc<RefCell<nuklear_rust::NkContext>>>) {
	unsafe {
		CTX_SYNC = context;
	}
}
pub fn set_drawer_async(drawer: Option<Arc<Mutex<Drawer>>>) {
	unsafe {
		DRAWER_ASYNC = drawer;
	}
}
pub fn set_context_async(context: Option<Arc<Mutex<nuklear_rust::NkContext>>>) {
	unsafe {
		CTX_ASYNC = context;
	}
}

fn register_window_class() -> Vec<u16> {
    unsafe {
    	let class_name = OsStr::new("NuklearWindowClass").encode_wide().chain(Some(0).into_iter()).collect::<Vec<_>>();

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
	
	let mut rect = winapi::RECT { left: 0, top:0, right: width, bottom: height };
    let style = winapi::WS_OVERLAPPEDWINDOW;
    let exstyle = winapi::WS_EX_APPWINDOW;
    
    user32::AdjustWindowRectEx(&mut rect, style, winapi::FALSE, exstyle);
    let window_name = OsStr::new(window_name).encode_wide().chain(Some(0).into_iter()).collect::<Vec<_>>();
	
    user32::CreateWindowExW(exstyle,
        class_name.as_ptr(),
        window_name.as_ptr() as winapi::LPCWSTR,
        style | winapi::WS_VISIBLE, winapi::CW_USEDEFAULT, winapi::CW_USEDEFAULT,
		rect.right - rect.left, rect.bottom - rect.top,
        ptr::null_mut(),
        ptr::null_mut(), 
        kernel32::GetModuleHandleW(ptr::null()),
        ptr::null_mut()
    )
}

unsafe extern "system" fn callback(wnd: winapi::HWND, msg: winapi::UINT, wparam: winapi::WPARAM, lparam: winapi::LPARAM) -> winapi::LRESULT {
	match msg {
		winapi::WM_DESTROY => {
			user32::PostQuitMessage(0);
			return 0;
		}
		_ => {
			if CTX_SYNC.is_some() && DRAWER_SYNC.is_some() {
				let mut nk_ctx = CTX_SYNC.as_ref().unwrap().borrow_mut();
				let mut drawer = DRAWER_SYNC.as_ref().unwrap().borrow_mut();
				
				if drawer.handle_event(&mut *nk_ctx, wnd, msg, wparam, lparam) {
					return 0;
				}
			} else if CTX_ASYNC.is_some() && DRAWER_ASYNC.is_some() {
				let mut nk_ctx = CTX_ASYNC.as_ref().unwrap().lock().unwrap();
				let mut drawer = DRAWER_ASYNC.as_ref().unwrap().lock().unwrap();
				
				if drawer.handle_event(&mut *nk_ctx, wnd, msg, wparam, lparam) {
					return 0;
				}
			} else {
				// println!("no contexts");
			}
		}
	}
	
	user32::DefWindowProcW(wnd, msg, wparam, lparam)
}

static mut CTX_SYNC: Option<Rc<RefCell<nuklear_rust::NkContext>>> = None;
static mut DRAWER_SYNC: Option<Rc<RefCell<Drawer>>> = None;
static mut CTX_ASYNC: Option<Arc<Mutex<nuklear_rust::NkContext>>> = None;
static mut DRAWER_ASYNC: Option<Arc<Mutex<Drawer>>> = None;