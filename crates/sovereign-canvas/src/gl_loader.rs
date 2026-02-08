//! GL proc address loader via libepoxy (or EGL fallback).
//! This module contains the only `unsafe` code in sovereign-canvas.

use std::ffi::{c_void, CString};

extern "C" {
    fn dlopen(filename: *const i8, flags: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const i8) -> *mut c_void;
    fn dlerror() -> *const i8;
}
const RTLD_LAZY: i32 = 1;

type GlGetProcAddr = unsafe extern "C" fn(*const i8) -> *const c_void;

/// Attempt to find a GL proc address function (epoxy preferred, EGL fallback).
///
/// # Safety
/// Uses dlopen/dlsym FFI. Must only be called after GTK has initialized GL.
pub(crate) unsafe fn load_gl_proc_address() -> Option<GlGetProcAddr> {
    dlerror(); // clear previous error

    // Strategy 1: epoxy in current process (GTK4 links against it)
    let handle = dlopen(std::ptr::null(), RTLD_LAZY);
    if !handle.is_null() {
        let sym_name = CString::new("epoxy_get_proc_address").unwrap();
        let sym = dlsym(handle, sym_name.as_ptr());
        if !sym.is_null() {
            tracing::debug!("Found epoxy_get_proc_address in process");
            return Some(std::mem::transmute(sym));
        }
    }

    // Strategy 2: dlopen libepoxy explicitly
    for lib in &["libepoxy.so.0", "libepoxy.so"] {
        let lib_name = CString::new(*lib).unwrap();
        dlerror();
        let handle = dlopen(lib_name.as_ptr(), RTLD_LAZY);
        if handle.is_null() {
            continue;
        }
        let sym_name = CString::new("epoxy_get_proc_address").unwrap();
        let sym = dlsym(handle, sym_name.as_ptr());
        if !sym.is_null() {
            tracing::debug!("Found epoxy_get_proc_address via {}", lib);
            return Some(std::mem::transmute(sym));
        }
    }

    // Strategy 3: eglGetProcAddress fallback
    let lib_name = CString::new("libEGL.so.1").unwrap();
    dlerror();
    let handle = dlopen(lib_name.as_ptr(), RTLD_LAZY);
    if !handle.is_null() {
        let sym_name = CString::new("eglGetProcAddress").unwrap();
        let sym = dlsym(handle, sym_name.as_ptr());
        if !sym.is_null() {
            tracing::debug!("Using eglGetProcAddress as fallback");
            return Some(std::mem::transmute(sym));
        }
    }

    tracing::error!("Could not find any GL proc address function");
    None
}
