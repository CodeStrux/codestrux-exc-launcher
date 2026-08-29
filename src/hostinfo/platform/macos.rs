//! macOS-specific system-info gathering (fields not covered by `sysinfo`).

mod iokit_ffi;

use std::ffi::{c_void, CString};
use std::sync::OnceLock;

/// 1-minute load average via `libc::getloadavg`.
pub fn load_avg() -> Option<f64> {
    let mut loads: [f64; 3] = [0.0; 3];
    // SAFETY: `loads` is a valid 3-element buffer for the call to write into;
    // `getloadavg` never writes past the count we pass.
    let got = unsafe { libc::getloadavg(loads.as_mut_ptr(), 3) };
    if got <= 0 {
        None
    } else {
        Some(loads[0])
    }
}

/// Not a macOS concept — `load_avg()` is the native metric here.
pub fn cpu_percent() -> Option<f32> {
    None
}

/// `vm.swapusage` reports `xsu_total`/`xsu_used` in bytes via a fixed-size
/// C struct with no `libc`-crate equivalent, so it's declared locally.
#[repr(C)]
struct XswUsage {
    xsu_total: u64,
    xsu_avail: u64,
    xsu_used: u64,
    xsu_pagesize: u32,
    xsu_encrypted: i32,
}

pub fn swap_info() -> Option<(u64, u64)> {
    let name = CString::new("vm.swapusage").ok()?;
    let mut usage = XswUsage { xsu_total: 0, xsu_avail: 0, xsu_used: 0, xsu_pagesize: 0, xsu_encrypted: 0 };
    let mut size = std::mem::size_of::<XswUsage>();
    // SAFETY: `usage`/`size` describe a valid, correctly-sized output buffer;
    // no new value is being set (newp/newlen are null/0).
    let rc = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            &mut usage as *mut XswUsage as *mut c_void,
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 {
        return None;
    }
    const MB: u64 = 1024 * 1024;
    Some((usage.xsu_used / MB, usage.xsu_total / MB))
}

pub fn local_time_string() -> Option<String> {
    // SAFETY: `time(null)` just reads the clock, no output pointer to
    // validate.
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: `tm` is a valid, correctly-sized output struct; `localtime_r`
    // fully initializes it on success and we only read it after checking
    // the return value.
    let rc = unsafe { libc::localtime_r(&now, &mut tm) };
    if rc.is_null() {
        return None;
    }
    let offset_mins = tm.tm_gmtoff / 60;
    let sign = if offset_mins >= 0 { '+' } else { '-' };
    Some(format!(
        "{:02}:{:02}:{:02} (UTC{sign}{:02}:{:02})",
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec,
        (offset_mins.abs()) / 60,
        (offset_mins.abs()) % 60,
    ))
}

// --- Battery, via IOKit power sources ---

use iokit_ffi::*;

fn cf_string(s: &str) -> Option<CFStringRef> {
    let c = CString::new(s).ok()?;
    // SAFETY: `c` is a valid, NUL-terminated C string alive for this call;
    // CFStringCreateWithCString copies its contents into the returned
    // CFStringRef, which we own and must CFRelease.
    let cf = unsafe { CFStringCreateWithCString(kCFAllocatorDefault, c.as_ptr(), K_CF_STRING_ENCODING_UTF8) };
    if cf.is_null() { None } else { Some(cf) }
}

/// Look up an `i32` value stored under `key` in `dict`. Returns `None` if
/// the key is absent or isn't a CFNumber.
fn dict_get_i32(dict: CFDictionaryRef, key: &str) -> Option<i32> {
    let cf_key = cf_string(key)?;
    // SAFETY: `dict`/`cf_key` are both valid CF objects for the duration of
    // this call.
    let value = unsafe { CFDictionaryGetValue(dict, cf_key) };
    // SAFETY: `cf_key` was created above and is owned by us.
    unsafe { CFRelease(cf_key) };
    if value.is_null() {
        return None;
    }
    let mut out: i32 = 0;
    // SAFETY: `value` was just returned as non-null by CFDictionaryGetValue;
    // `out` is a valid i32 output slot matching K_CF_NUMBER_SINT32_TYPE.
    let ok = unsafe { CFNumberGetValue(value, K_CF_NUMBER_SINT32_TYPE, &mut out as *mut i32 as *mut c_void) };
    if ok != 0 { Some(out) } else { None }
}

fn dict_get_bool(dict: CFDictionaryRef, key: &str) -> Option<bool> {
    let cf_key = cf_string(key)?;
    // SAFETY: see `dict_get_i32`.
    let value = unsafe { CFDictionaryGetValue(dict, cf_key) };
    unsafe { CFRelease(cf_key) };
    if value.is_null() {
        return None;
    }
    // SAFETY: `value` is non-null and, for the "Is Charging" key, always a
    // CFBoolean per IOKit's documented power-source dictionary schema.
    Some(unsafe { CFBooleanGetValue(value) } != 0)
}

pub fn battery() -> Option<(u8, bool)> {
    // SAFETY: no arguments; returns an owned CFTypeRef we must CFRelease.
    let blob = unsafe { IOPSCopyPowerSourcesInfo() };
    if blob.is_null() {
        return None;
    }
    // SAFETY: `blob` is a valid, non-null CFTypeRef from the call above,
    // alive for the duration of this function.
    let list = unsafe { IOPSCopyPowerSourcesList(blob) };
    if list.is_null() {
        unsafe { CFRelease(blob) };
        return None;
    }

    // SAFETY: `list` is a valid, non-null CFArrayRef.
    let count = unsafe { CFArrayGetCount(list) };
    let mut result = None;
    for i in 0..count {
        // SAFETY: `i` is within `[0, count)`, `list` is valid for this loop.
        let ps = unsafe { CFArrayGetValueAtIndex(list, i) };
        if ps.is_null() {
            continue;
        }
        // SAFETY: `blob`/`ps` are both valid CF objects; the returned
        // dictionary is not owned by us (Apple's "Get" convention).
        let desc = unsafe { IOPSGetPowerSourceDescription(blob, ps) };
        if desc.is_null() {
            continue;
        }
        let current = dict_get_i32(desc, "Current Capacity");
        let max = dict_get_i32(desc, "Max Capacity");
        if let (Some(current), Some(max)) = (current, max)
            && max > 0
        {
            let pct = ((current as f64 / max as f64) * 100.0).round().clamp(0.0, 100.0) as u8;
            let charging = dict_get_bool(desc, "Is Charging").unwrap_or(false);
            result = Some((pct, charging));
            break;
        }
    }

    // SAFETY: both are valid, owned CFTypeRefs obtained above.
    unsafe {
        CFRelease(list);
        CFRelease(blob);
    }
    result
}

// --- GPU name, via IOKit service registry ---

pub fn gpu_name() -> Option<String> {
    static CACHE: OnceLock<Option<String>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            std::process::Command::new("system_profiler")
                .args(["SPDisplaysDataType", "-json"])
                .output()
                .ok()
                .filter(|out| out.status.success())
                .and_then(|out| parse_gpu_name_from_system_profiler(&String::from_utf8_lossy(&out.stdout)))
        })
        .clone()
}

/// `system_profiler`'s JSON output nests the chip name under
/// `SPDisplaysDataType[0].sppci_model` (or `_name` as a fallback on older
/// macOS versions). Parsed with a small hand-rolled scan instead of adding a
/// JSON crate dependency solely for this one field.
fn parse_gpu_name_from_system_profiler(json: &str) -> Option<String> {
    for key in ["\"sppci_model\"", "\"_name\""] {
        if let Some(pos) = json.find(key) {
            let after_key = &json[pos + key.len()..];
            let colon = after_key.find(':')?;
            let rest = after_key[colon + 1..].trim_start();
            if let Some(rest) = rest.strip_prefix('"') {
                let end = rest.find('"')?;
                let name = rest[..end].trim();
                if !name.is_empty() {
                    return Some(name.to_string());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gpu_model_from_system_profiler_json() {
        let json = r#"{"SPDisplaysDataType":[{"sppci_model":"Apple M4 Pro","_name":"Apple M4 Pro"}]}"#;
        assert_eq!(parse_gpu_name_from_system_profiler(json).as_deref(), Some("Apple M4 Pro"));
    }

    #[test]
    fn missing_fields_yield_none() {
        assert_eq!(parse_gpu_name_from_system_profiler("{}"), None);
    }
}
