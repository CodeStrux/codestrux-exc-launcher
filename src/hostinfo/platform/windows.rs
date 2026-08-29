//! Windows-specific system-info gathering (fields not covered by `sysinfo`).
//! Raw kernel32 FFI, hand-declared since none of these structs/functions
//! have a `libc`-crate equivalent and no `windows`-crate dependency is
//! available.
//!
//! Not run against real Windows hardware in this session — verified with
//! `cargo check --target x86_64-pc-windows-msvc` and code review only.
#![allow(non_snake_case, non_camel_case_types)]

use std::sync::OnceLock;

type BOOL = i32;
type DWORD = u32;
type WORD = u16;
type LONG = i32;

#[repr(C)]
struct FILETIME {
    dw_low_date_time: DWORD,
    dw_high_date_time: DWORD,
}

impl FILETIME {
    fn as_u64(&self) -> u64 {
        ((self.dw_high_date_time as u64) << 32) | self.dw_low_date_time as u64
    }
}

#[repr(C)]
struct MEMORYSTATUSEX {
    dw_length: DWORD,
    dw_memory_load: DWORD,
    ull_total_phys: u64,
    ull_avail_phys: u64,
    ull_total_page_file: u64,
    ull_avail_page_file: u64,
    ull_total_virtual: u64,
    ull_avail_virtual: u64,
    ull_avail_extended_virtual: u64,
}

#[repr(C)]
struct SYSTEM_POWER_STATUS {
    ac_line_status: u8,
    battery_flag: u8,
    battery_life_percent: u8,
    reserved1: u8,
    battery_life_time: DWORD,
    battery_full_life_time: DWORD,
}

#[repr(C)]
struct SYSTEMTIME {
    w_year: WORD,
    w_month: WORD,
    w_day_of_week: WORD,
    w_day: WORD,
    w_hour: WORD,
    w_minute: WORD,
    w_second: WORD,
    w_milliseconds: WORD,
}

#[repr(C)]
struct TIME_ZONE_INFORMATION {
    bias: LONG,
    standard_name: [WORD; 32],
    standard_date: SYSTEMTIME,
    standard_bias: LONG,
    daylight_name: [WORD; 32],
    daylight_date: SYSTEMTIME,
    daylight_bias: LONG,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetSystemTimes(lpIdleTime: *mut FILETIME, lpKernelTime: *mut FILETIME, lpUserTime: *mut FILETIME) -> BOOL;
    fn GlobalMemoryStatusEx(lpBuffer: *mut MEMORYSTATUSEX) -> BOOL;
    fn GetSystemPowerStatus(lpSystemPowerStatus: *mut SYSTEM_POWER_STATUS) -> BOOL;
    fn GetLocalTime(lpSystemTime: *mut SYSTEMTIME);
    fn GetTimeZoneInformation(lpTimeZoneInformation: *mut TIME_ZONE_INFORMATION) -> DWORD;
}

const TIME_ZONE_ID_DAYLIGHT: DWORD = 2;

/// Windows has no load-average concept — `cpu_percent()` is the native
/// metric here instead. No forced parity.
pub fn load_avg() -> Option<f64> {
    None
}

fn get_system_times() -> Option<(u64, u64, u64)> {
    let mut idle: FILETIME = unsafe { std::mem::zeroed() };
    let mut kernel: FILETIME = unsafe { std::mem::zeroed() };
    let mut user: FILETIME = unsafe { std::mem::zeroed() };
    // SAFETY: all three pointers reference valid, correctly-sized FILETIME
    // structs that GetSystemTimes fully populates on success (checked below).
    let ok = unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) };
    if ok == 0 {
        None
    } else {
        Some((idle.as_u64(), kernel.as_u64(), user.as_u64()))
    }
}

/// Two `GetSystemTimes` samples ~30ms apart. `lpKernelTime` includes idle
/// time on Windows, so total CPU time is kernel + user and busy time is
/// that total minus the idle delta.
pub fn cpu_percent() -> Option<f32> {
    let (idle1, kernel1, user1) = get_system_times()?;
    std::thread::sleep(std::time::Duration::from_millis(30));
    let (idle2, kernel2, user2) = get_system_times()?;

    let idle_delta = idle2.saturating_sub(idle1);
    let total_delta = kernel2.saturating_sub(kernel1) + user2.saturating_sub(user1);
    if total_delta == 0 {
        return None;
    }
    let busy = total_delta.saturating_sub(idle_delta);
    Some((busy as f64 / total_delta as f64 * 100.0) as f32)
}

pub fn swap_info() -> Option<(u64, u64)> {
    let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    status.dw_length = std::mem::size_of::<MEMORYSTATUSEX>() as DWORD;
    // SAFETY: `status.dw_length` is set to the struct's real size as the
    // Win32 contract requires before this call; `status` is a valid,
    // correctly-sized output buffer.
    let ok = unsafe { GlobalMemoryStatusEx(&mut status) };
    if ok == 0 {
        return None;
    }
    const MB: u64 = 1024 * 1024;
    let total = status.ull_total_page_file / MB;
    let avail = status.ull_avail_page_file / MB;
    Some((total.saturating_sub(avail), total))
}

pub fn battery() -> Option<(u8, bool)> {
    let mut status: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
    // SAFETY: `status` is a valid, correctly-sized output struct.
    let ok = unsafe { GetSystemPowerStatus(&mut status) };
    if ok == 0 {
        return None;
    }
    const BATTERY_FLAG_NO_SYSTEM_BATTERY: u8 = 128;
    const BATTERY_FLAG_UNKNOWN: u8 = 255;
    const BATTERY_FLAG_CHARGING: u8 = 8;
    if status.battery_life_percent == BATTERY_FLAG_UNKNOWN
        || status.battery_flag == BATTERY_FLAG_NO_SYSTEM_BATTERY
        || status.battery_flag == BATTERY_FLAG_UNKNOWN
    {
        return None;
    }
    let charging = status.battery_flag & BATTERY_FLAG_CHARGING != 0;
    Some((status.battery_life_percent, charging))
}

pub fn local_time_string() -> Option<String> {
    let mut local: SYSTEMTIME = unsafe { std::mem::zeroed() };
    // SAFETY: `local` is a valid, correctly-sized output struct;
    // GetLocalTime always succeeds (no error return to check).
    unsafe { GetLocalTime(&mut local) };

    let mut tz: TIME_ZONE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: `tz` is a valid, correctly-sized output struct.
    let tz_id = unsafe { GetTimeZoneInformation(&mut tz) };

    // `Bias` is minutes *west* of UTC; local = UTC - bias, so the
    // UTC-relative (east-positive) offset is the negation, adjusted for DST.
    let bias = tz.bias + if tz_id == TIME_ZONE_ID_DAYLIGHT { tz.daylight_bias } else { 0 };
    let offset_mins = -bias;
    let sign = if offset_mins >= 0 { '+' } else { '-' };
    Some(format!(
        "{:02}:{:02}:{:02} (UTC{sign}{:02}:{:02})",
        local.w_hour,
        local.w_minute,
        local.w_second,
        offset_mins.abs() / 60,
        offset_mins.abs() % 60,
    ))
}

pub fn gpu_name() -> Option<String> {
    static CACHE: OnceLock<Option<String>> = OnceLock::new();
    CACHE.get_or_init(gpu_name_uncached).clone()
}

/// `wmic` was removed from Windows 11 as of the 2026 servicing updates, so
/// this shells out to PowerShell's CIM cmdlet instead of raw DXGI COM (which
/// would need hand-rolled vtable calls with no crate support available).
fn gpu_name_uncached() -> Option<String> {
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "(Get-CimInstance Win32_VideoController | Select-Object -First 1 -ExpandProperty Name)",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if name.is_empty() { None } else { Some(name) }
}
