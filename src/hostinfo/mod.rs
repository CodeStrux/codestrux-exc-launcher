pub mod background;
pub mod panel;
mod platform;

use sysinfo::{Disks, System};

#[derive(Debug, Clone, Default)]
pub struct SystemInfo {
    pub user_at_host: String,
    pub os_display: String,
    pub host_model: String,
    pub kernel_version: String,
    pub uptime_secs: u64,
    pub mem_used_mb: u64,
    pub mem_total_mb: u64,
    pub disk_used_gb: u64,
    pub disk_total_gb: u64,
    pub disk_pct: u8,
    pub process_count: usize,

    // Tier 1 — synchronous, computed once here in gather(), must stay free.
    pub cpu_model: Option<String>,
    pub cpu_cores: Option<usize>,
    pub load_avg_1m: Option<f64>,
    /// Windows-only equivalent of `load_avg_1m` (no native load-average
    /// concept there) — see `platform::cpu_percent`.
    pub cpu_percent: Option<f32>,
    pub swap_used_mb: Option<u64>,
    pub swap_total_mb: Option<u64>,
    pub local_ip: Option<String>,
    pub battery_pct: Option<u8>,
    pub battery_charging: Option<bool>,
    pub shell: Option<String>,
    pub terminal: Option<String>,

    // Tier 2 — cheap, reuses data already being collected.
    pub gpu_name: Option<String>,

    // Tier 3 — background-refreshed; see `background::Tier3Handle`.
    pub tier3: background::Tier3Handle,
}

#[cfg(target_os = "macos")]
fn host_model() -> String {
    if let Ok(out) = std::process::Command::new("sysctl").args(["-n", "hw.model"]).output()
        && out.status.success()
    {
        let model = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !model.is_empty() {
            return model;
        }
    }
    System::host_name().unwrap_or_else(|| "unknown".to_string())
}

#[cfg(not(target_os = "macos"))]
fn host_model() -> String {
    System::host_name().unwrap_or_else(|| "unknown".to_string())
}

fn current_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "user".to_string())
}

/// Determined without any per-OS branching: "connect" a UDP socket to a
/// public address (no packet is actually sent for UDP `connect`) and read
/// back which local interface/address the kernel would route it through.
fn local_ip() -> Option<String> {
    use std::net::UdpSocket;
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    socket.local_addr().ok().map(|addr| addr.ip().to_string())
}

fn shell_and_terminal() -> (Option<String>, Option<String>) {
    let shell = std::env::var("SHELL").ok().filter(|s| !s.is_empty());
    let terminal = std::env::var("TERM_PROGRAM")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("WT_SESSION").ok().map(|_| "Windows Terminal".to_string()))
        .or_else(|| std::env::var("PSModulePath").ok().map(|_| "PowerShell".to_string()))
        .or_else(|| std::env::var("COMSPEC").ok().map(|_| "cmd".to_string()));
    (shell, terminal)
}

pub fn gather() -> SystemInfo {
    let mut sys = System::new_all();
    sys.refresh_all();

    let hostname = System::host_name().unwrap_or_else(|| "unknown".to_string());
    let user_at_host = format!("{}@{}", current_user(), hostname);

    let mem_total_mb = sys.total_memory() / 1024 / 1024;
    let mem_used_mb = sys.used_memory() / 1024 / 1024;

    let disks = Disks::new_with_refreshed_list();
    let root_disk = disks
        .list()
        .iter()
        .find(|d| d.mount_point().to_string_lossy() == "/")
        .or_else(|| disks.list().first());

    let (disk_total_gb, disk_used_gb, disk_pct) = match root_disk {
        Some(d) => {
            let total = d.total_space();
            let available = d.available_space();
            let used = total.saturating_sub(available);
            let total_gb = total / 1024 / 1024 / 1024;
            let used_gb = used / 1024 / 1024 / 1024;
            let pct = if total > 0 { ((used as f64 / total as f64) * 100.0).round() as u8 } else { 0 };
            (total_gb, used_gb, pct)
        }
        None => (0, 0, 0),
    };

    let os_display = System::long_os_version().unwrap_or_else(|| {
        format!(
            "{} {}",
            System::name().unwrap_or_else(|| "unknown OS".to_string()),
            System::os_version().unwrap_or_default()
        )
        .trim()
        .to_string()
    });

    let cpus = sys.cpus();
    let cpu_model = cpus.first().map(|c| c.brand().trim().to_string()).filter(|s| !s.is_empty());
    let cpu_cores = if cpus.is_empty() { None } else { Some(cpus.len()) };

    let swap = platform::swap_info();
    let battery = platform::battery();
    let (shell, terminal) = shell_and_terminal();

    SystemInfo {
        user_at_host,
        os_display,
        host_model: host_model(),
        kernel_version: System::kernel_version().unwrap_or_default(),
        uptime_secs: System::uptime(),
        mem_used_mb,
        mem_total_mb,
        disk_used_gb,
        disk_total_gb,
        disk_pct,
        process_count: sys.processes().len(),
        cpu_model,
        cpu_cores,
        load_avg_1m: platform::load_avg(),
        cpu_percent: platform::cpu_percent(),
        swap_used_mb: swap.map(|(used, _)| used),
        swap_total_mb: swap.map(|(_, total)| total),
        local_ip: local_ip(),
        battery_pct: battery.map(|(pct, _)| pct),
        battery_charging: battery.map(|(_, charging)| charging),
        shell,
        terminal,
        gpu_name: platform::gpu_name(),
        tier3: background::Tier3Handle::default(),
    }
}

pub fn format_uptime(secs: u64) -> String {
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let minutes = (secs % 3600) / 60;
    if days > 0 {
        format!("{days}d {hours}h {minutes}m")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_uptime_various() {
        assert_eq!(format_uptime(59), "0m");
        assert_eq!(format_uptime(3661), "1h 1m");
        assert_eq!(format_uptime(2 * 86400 + 4 * 3600 + 24 * 60), "2d 4h 24m");
    }

    #[test]
    fn gather_produces_nonempty_fields() {
        let info = gather();
        assert!(!info.user_at_host.is_empty());
        assert!(!info.os_display.is_empty());
    }
}
