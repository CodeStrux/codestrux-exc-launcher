//! Raw CoreFoundation/IOKit declarations used by battery status and GPU-name
//! lookups. Kept isolated in its own file since it's the largest unsafe
//! surface in the codebase — safe wrapper functions live in `super` (i.e.
//! `platform::macos`), nothing outside this file touches these signatures
//! directly.
#![allow(non_camel_case_types, non_upper_case_globals, dead_code)]

use std::ffi::c_void;
use std::os::raw::{c_char, c_int, c_long, c_uchar};

pub type CFIndex = c_long;
pub type CFTypeRef = *const c_void;
pub type CFStringRef = *const c_void;
pub type CFDictionaryRef = *const c_void;
pub type CFArrayRef = *const c_void;
pub type CFAllocatorRef = *const c_void;
pub type CFNumberRef = *const c_void;
pub type CFNumberType = c_int;
pub type Boolean = c_uchar;
pub type CFStringEncoding = u32;

pub const K_CF_STRING_ENCODING_UTF8: CFStringEncoding = 0x0800_0100;
pub const K_CF_NUMBER_SINT32_TYPE: CFNumberType = 3;

unsafe extern "C" {
    pub static kCFAllocatorDefault: CFAllocatorRef;

    pub fn CFRelease(cf: CFTypeRef);
    pub fn CFArrayGetCount(array: CFArrayRef) -> CFIndex;
    pub fn CFArrayGetValueAtIndex(array: CFArrayRef, idx: CFIndex) -> *const c_void;
    pub fn CFStringCreateWithCString(
        alloc: CFAllocatorRef,
        c_str: *const c_char,
        encoding: CFStringEncoding,
    ) -> CFStringRef;
    pub fn CFDictionaryGetValue(dict: CFDictionaryRef, key: *const c_void) -> *const c_void;
    pub fn CFNumberGetValue(number: CFNumberRef, the_type: CFNumberType, value_ptr: *mut c_void) -> Boolean;
    pub fn CFBooleanGetValue(boolean: *const c_void) -> Boolean;
}

unsafe extern "C" {
    /// Returns an opaque "blob" (owned — must `CFRelease`) describing the
    /// current power sources.
    pub fn IOPSCopyPowerSourcesInfo() -> CFTypeRef;
    /// Returns the list of power sources in `blob` (owned — must
    /// `CFRelease`).
    pub fn IOPSCopyPowerSourcesList(blob: CFTypeRef) -> CFArrayRef;
    /// Returns the description dictionary for one power source. Per Apple's
    /// "Get" naming convention this is *not* owned by the caller — no
    /// release needed.
    pub fn IOPSGetPowerSourceDescription(blob: CFTypeRef, ps: CFTypeRef) -> CFDictionaryRef;
}
