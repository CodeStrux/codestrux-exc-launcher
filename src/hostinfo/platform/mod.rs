//! Per-OS system-info gathering for fields `sysinfo` doesn't cover (load
//! average, swap, battery, GPU name, wall-clock time). Every platform module
//! exposes the same function set so call sites in `hostinfo::gather()` never
//! need their own `#[cfg]` gating — a platform that lacks a given concept
//! (e.g. load average on Windows) just returns `None` from that function.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::*;

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
mod fallback;
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub use fallback::*;
