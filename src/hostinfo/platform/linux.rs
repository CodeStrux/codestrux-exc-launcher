//! Linux-specific system-info gathering (fields not covered by `sysinfo`).
//!
//! Not run against real Linux hardware in this session — verified with
//! `cargo check --target x86_64-unknown-linux-gnu` and code review only.

use std::sync::OnceLock;

// The `libc` crate only binds `getloadavg` for BSD-family targets, not
// glibc/musl Linux — even though both C libraries actually export it (glibc
// as a GNU extension, musl reading /proc/loadavg internally). Declare it
// ourselves rather than parsing /proc/loadavg by hand.
unsafe extern "C" {
    fn getloadavg(loadavg: *mut libc::c_double, nelem: libc::c_int) -> libc::c_int;
}

/// 1-minute load average via `getloadavg`.
pub fn load_avg() -> Option<f64> {
    let mut loads: [f64; 3] = [0.0; 3];
    // SAFETY: `loads` is a valid 3-element buffer for the call to write into;
    // `getloadavg` never writes past the count we pass.
    let got = unsafe { getloadavg(loads.as_mut_ptr(), 3) };
    if got <= 0 {
        None
    } else {
        Some(loads[0])
    }
}

/// Not tracked separately on Linux — `load_avg()` is the native metric here.
pub fn cpu_percent() -> Option<f32> {
    None
}

fn parse_meminfo_kb(line_rest: &str) -> Option<u64> {
    line_rest.trim().trim_end_matches("kB").trim().parse().ok()
}

pub fn swap_info() -> Option<(u64, u64)> {
    let content = std::fs::read_to_string("/proc/meminfo").ok()?;
    let mut total_kb = None;
    let mut free_kb = None;
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("SwapTotal:") {
            total_kb = parse_meminfo_kb(rest);
        } else if let Some(rest) = line.strip_prefix("SwapFree:") {
            free_kb = parse_meminfo_kb(rest);
        }
    }
    let (total_kb, free_kb) = (total_kb?, free_kb?);
    let used_kb = total_kb.saturating_sub(free_kb);
    Some((used_kb / 1024, total_kb / 1024))
}

pub fn battery() -> Option<(u8, bool)> {
    let entries = std::fs::read_dir("/sys/class/power_supply").ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with("BAT") {
            continue;
        }
        let path = entry.path();
        let Ok(capacity_raw) = std::fs::read_to_string(path.join("capacity")) else { continue };
        let Ok(pct) = capacity_raw.trim().parse::<u8>() else { continue };
        let charging = std::fs::read_to_string(path.join("status"))
            .map(|s| s.trim().eq_ignore_ascii_case("charging"))
            .unwrap_or(false);
        return Some((pct, charging));
    }
    None
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

/// PCI vendor IDs for the 3 GPU vendors worth naming; anything else is
/// skipped rather than guessed at.
fn vendor_name(pci_vendor_id: &str) -> Option<&'static str> {
    match pci_vendor_id.trim() {
        "0x10de" => Some("NVIDIA"),
        "0x1002" => Some("AMD"),
        "0x8086" => Some("Intel"),
        _ => None,
    }
}

fn gpu_name_from_sysfs() -> Option<String> {
    let entries = std::fs::read_dir("/sys/class/drm").ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // Only the top-level card nodes (card0, card1, ...) carry a
        // `device` symlink to the PCI device — the render/framebuffer
        // sub-nodes (card0-DP-1, etc.) don't.
        if !name.starts_with("card") || name.contains('-') {
            continue;
        }
        let device_dir = entry.path().join("device");
        let Ok(vendor) = std::fs::read_to_string(device_dir.join("vendor")) else { continue };
        let Ok(device) = std::fs::read_to_string(device_dir.join("device")) else { continue };
        if let Some(vendor_name) = vendor_name(&vendor) {
            return Some(format!("{vendor_name} ({})", device.trim()));
        }
    }
    None
}

fn gpu_name_from_lspci() -> Option<String> {
    let out = std::process::Command::new("lspci").arg("-mm").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .find(|line| line.contains("VGA") || line.contains("3D controller"))
        .map(|line| line.to_string())
}

pub fn gpu_name() -> Option<String> {
    static CACHE: OnceLock<Option<String>> = OnceLock::new();
    CACHE.get_or_init(|| gpu_name_from_sysfs().or_else(gpu_name_from_lspci)).clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_swap_totals_from_meminfo_format() {
        let sample = "MemTotal:       32821936 kB\nSwapTotal:       2097148 kB\nSwapFree:        1048574 kB\n";
        let mut total = None;
        let mut free = None;
        for line in sample.lines() {
            if let Some(rest) = line.strip_prefix("SwapTotal:") {
                total = parse_meminfo_kb(rest);
            } else if let Some(rest) = line.strip_prefix("SwapFree:") {
                free = parse_meminfo_kb(rest);
            }
        }
        assert_eq!(total, Some(2_097_148));
        assert_eq!(free, Some(1_048_574));
    }

    #[test]
    fn recognizes_the_three_gpu_vendor_ids() {
        assert_eq!(vendor_name("0x10de"), Some("NVIDIA"));
        assert_eq!(vendor_name("0x1002\n"), Some("AMD"));
        assert_eq!(vendor_name("0x8086"), Some("Intel"));
        assert_eq!(vendor_name("0x1234"), None);
    }
}
