//! No-op implementations for any target that isn't macOS/Linux/Windows, so
//! the crate still builds elsewhere — every field just reads as absent.

pub fn load_avg() -> Option<f64> {
    None
}

pub fn cpu_percent() -> Option<f32> {
    None
}

pub fn swap_info() -> Option<(u64, u64)> {
    None
}

pub fn battery() -> Option<(u8, bool)> {
    None
}

pub fn local_time_string() -> Option<String> {
    None
}

pub fn gpu_name() -> Option<String> {
    None
}
