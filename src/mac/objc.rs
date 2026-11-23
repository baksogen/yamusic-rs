//! Типы и вызовы objc runtime. objc_msgSend приводится к точной сигнатуре каждого
//! вызова: на arm64 вариадическое соглашение не используется.

use std::ffi::{c_char, c_void, CString};
use std::os::raw::{c_long, c_ulong};

pub(crate) type Id = *mut c_void;
pub(crate) type Sel = *mut c_void;

extern "C" {
    pub(crate) fn objc_getClass(name: *const c_char) -> Id;
    pub(crate) fn sel_registerName(name: *const c_char) -> Sel;
    pub(crate) fn objc_msgSend();
    #[cfg(target_arch = "x86_64")]
    pub(crate) fn objc_msgSend_stret();
    pub(crate) fn objc_allocateClassPair(superclass: Id, name: *const c_char, extra_bytes: usize) -> Id;
    pub(crate) fn class_addMethod(cls: Id, name: Sel, imp: *const c_void, types: *const c_char) -> bool;
    pub(crate) fn objc_registerClassPair(cls: Id);
}

pub(crate) fn c(s: &str) -> CString {
    CString::new(s).unwrap()
}

// objc_msgSend нужно приводить к точной сигнатуре каждого вызова (на arm64 variadic
// соглашение не используется). transmute между указателями на функции одного размера.
pub(crate) unsafe fn msg0(obj: Id, sel: &str) -> Id {
    let f: unsafe extern "C" fn(Id, Sel) -> Id = std::mem::transmute(objc_msgSend as unsafe extern "C" fn() as usize);
    f(obj, sel_registerName(c(sel).as_ptr()))
}

pub(crate) unsafe fn msg_set_ulong(obj: Id, sel: &str, v: c_ulong) {
    let f: unsafe extern "C" fn(Id, Sel, c_ulong) = std::mem::transmute(objc_msgSend as unsafe extern "C" fn() as usize);
    f(obj, sel_registerName(c(sel).as_ptr()), v);
}

pub(crate) unsafe fn msg_set_bool(obj: Id, sel: &str, v: bool) {
    let f: unsafe extern "C" fn(Id, Sel, bool) = std::mem::transmute(objc_msgSend as unsafe extern "C" fn() as usize);
    f(obj, sel_registerName(c(sel).as_ptr()), v);
}

pub(crate) unsafe fn msg_set_long(obj: Id, sel: &str, v: c_long) {
    let f: unsafe extern "C" fn(Id, Sel, c_long) = std::mem::transmute(objc_msgSend as unsafe extern "C" fn() as usize);
    f(obj, sel_registerName(c(sel).as_ptr()), v);
}

pub(crate) unsafe fn msg_set_double(obj: Id, sel: &str, v: f64) {
    let f: unsafe extern "C" fn(Id, Sel, f64) = std::mem::transmute(objc_msgSend as unsafe extern "C" fn() as usize);
    f(obj, sel_registerName(c(sel).as_ptr()), v);
}

pub(crate) unsafe fn msg_responds(obj: Id, sel: Sel) -> bool {
    let f: unsafe extern "C" fn(Id, Sel, Sel) -> bool = std::mem::transmute(objc_msgSend as unsafe extern "C" fn() as usize);
    f(obj, sel_registerName(c("respondsToSelector:").as_ptr()), sel)
}

pub(crate) unsafe fn msg_string(cls: Id, s: &str) -> Id {
    let f: unsafe extern "C" fn(Id, Sel, *const c_char) -> Id = std::mem::transmute(objc_msgSend as unsafe extern "C" fn() as usize);
    f(cls, sel_registerName(c("stringWithUTF8String:").as_ptr()), c(s).as_ptr())
}

pub(crate) unsafe fn msg_double(obj: Id, sel: &str) -> f64 {
    let f: unsafe extern "C" fn(Id, Sel) -> f64 = std::mem::transmute(objc_msgSend as unsafe extern "C" fn() as usize);
    f(obj, sel_registerName(c(sel).as_ptr()))
}

pub(crate) unsafe fn msg_set_id(obj: Id, sel: &str, arg: Id) {
    let f: unsafe extern "C" fn(Id, Sel, Id) = std::mem::transmute(objc_msgSend as unsafe extern "C" fn() as usize);
    f(obj, sel_registerName(c(sel).as_ptr()), arg);
}
