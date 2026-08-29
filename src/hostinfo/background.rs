//! Tier 3 fields (public IP, pending package updates, network throughput):
//! expensive/networked, so they're fetched off the render path and handed
//! into the panel through a small shared snapshot instead of being computed
//! synchronously in `gather()` like every other field.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How often the persistent background thread (used while the picker is
/// open) refreshes public IP and network throughput.
const FAST_TICK: Duration = Duration::from_secs(30);
/// Pending-updates checks run their own package-manager command, which can
/// itself take multiple seconds — only run it every 60th fast tick (~30 min).
const SLOW_TICK_MULTIPLE: u32 = 60;

#[derive(Debug, Clone, Default)]
pub struct Tier3Snapshot {
    pub public_ip: Option<String>,
    pub pending_updates: Option<usize>,
    pub net_rx_bps: Option<u64>,
    pub net_tx_bps: Option<u64>,
    /// False until the first background pass completes — lets the panel
    /// distinguish "not fetched yet" from "fetched, nothing available".
    pub ready: bool,
}

/// A cheap-to-clone handle to a shared snapshot. `SystemInfo` carries one of
/// these; the picker's background thread writes into it, and `panel::fields`
/// reads a clone of the current value on every render.
#[derive(Debug, Clone, Default)]
pub struct Tier3Handle(Arc<Mutex<Tier3Snapshot>>);

impl Tier3Handle {
    /// Read the current snapshot. Recovers from a poisoned mutex instead of
    /// propagating the panic — a stale info panel must never crash the
    /// picker.
    pub fn snapshot(&self) -> Tier3Snapshot {
        match self.0.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    pub(crate) fn set(&self, snapshot: Tier3Snapshot) {
        match self.0.lock() {
            Ok(mut guard) => *guard = snapshot,
            Err(poisoned) => *poisoned.into_inner() = snapshot,
        }
    }
}

/// Spawns the persistent background refresh loop used while the picker is
/// open. Detached — never joined, since the only side effect is writing
/// into `handle`'s mutex and the thread simply stops existing on process
/// exit (no unflushed I/O to lose).
pub fn spawn_refresh_thread(handle: Tier3Handle) {
    std::thread::spawn(move || {
        let mut networks = sysinfo::Networks::new_with_refreshed_list();
        let mut last_tick = Instant::now();
        let mut tick: u32 = 0;
        loop {
            let public_ip = fetch_public_ip();

            networks.refresh(true);
            let elapsed = last_tick.elapsed().as_secs_f64().max(1.0);
            last_tick = Instant::now();
            let (rx, tx) = networks
                .list()
                .values()
                .fold((0u64, 0u64), |(rx, tx), n| (rx + n.received(), tx + n.transmitted()));

            let pending_updates = if tick.is_multiple_of(SLOW_TICK_MULTIPLE) {
                fetch_pending_updates()
            } else {
                handle.snapshot().pending_updates
            };

            handle.set(Tier3Snapshot {
                public_ip,
                pending_updates,
                net_rx_bps: Some((rx as f64 / elapsed) as u64),
                net_tx_bps: Some((tx as f64 / elapsed) as u64),
                ready: true,
            });

            tick = tick.wrapping_add(1);
            std::thread::sleep(FAST_TICK);
        }
    });
}

/// One-shot fetch for `exc sysinfo` (which exits immediately, so a
/// persistent thread doesn't make sense). Bounded by `timeout` — on
/// timeout, returns a `Tier3Snapshot::default()` (fields omitted, not
/// ready) rather than blocking the CLI's core purpose indefinitely.
/// Network throughput needs two time-separated samples, which doesn't fit
/// a quick one-shot call, so it's left `None` here.
pub fn fetch_tier3_bounded(timeout: Duration) -> Tier3Snapshot {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let snapshot = Tier3Snapshot {
            public_ip: fetch_public_ip(),
            pending_updates: fetch_pending_updates(),
            net_rx_bps: None,
            net_tx_bps: None,
            ready: true,
        };
        let _ = tx.send(snapshot);
    });
    rx.recv_timeout(timeout).unwrap_or_default()
}

/// Plain-HTTP GET to an IP-echo endpoint over a raw `TcpStream` — no TLS
/// crate is available, so this deliberately avoids HTTPS.
fn fetch_public_ip() -> Option<String> {
    let addr = "api.ipify.org:80".to_socket_addrs().ok()?.next()?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(3))).ok()?;
    stream.set_write_timeout(Some(Duration::from_secs(3))).ok()?;
    stream.write_all(b"GET / HTTP/1.1\r\nHost: api.ipify.org\r\nConnection: close\r\n\r\n").ok()?;

    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;
    let body = response.split("\r\n\r\n").nth(1)?.trim();
    let looks_like_an_address = !body.is_empty()
        && body.len() <= 45 // max textual length of an IPv6 address
        && body.chars().all(|c| c.is_ascii_hexdigit() || c == '.' || c == ':');
    if looks_like_an_address { Some(body.to_string()) } else { None }
}

#[cfg(target_os = "macos")]
fn fetch_pending_updates() -> Option<usize> {
    let out = std::process::Command::new("brew").arg("outdated").output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).lines().filter(|l| !l.trim().is_empty()).count())
}

#[cfg(target_os = "linux")]
fn fetch_pending_updates() -> Option<usize> {
    use std::path::Path;

    if Path::new("/usr/bin/apt").exists() || Path::new("/usr/bin/apt-get").exists() {
        let out = std::process::Command::new("apt").args(["list", "--upgradable"]).output().ok()?;
        if !out.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&out.stdout);
        return Some(text.lines().filter(|l| !l.trim().is_empty() && !l.starts_with("Listing")).count());
    }
    if Path::new("/usr/bin/dnf").exists() {
        let out = std::process::Command::new("dnf").arg("check-update").output().ok()?;
        // dnf exits 100 (not 0) when updates ARE available, so status alone
        // can't gate this the way it does for apt/pacman.
        if !matches!(out.status.code(), Some(0) | Some(100)) {
            return None;
        }
        let text = String::from_utf8_lossy(&out.stdout);
        return Some(text.lines().filter(|l| l.trim().split_whitespace().count() >= 3).count());
    }
    if Path::new("/usr/bin/pacman").exists() {
        let out = std::process::Command::new("pacman").args(["-Qu"]).output().ok()?;
        if !out.status.success() {
            return None;
        }
        return Some(String::from_utf8_lossy(&out.stdout).lines().filter(|l| !l.trim().is_empty()).count());
    }
    None
}

/// Never uses `wmic` (removed from Windows 11 as of the 2026 servicing
/// updates) — shells out to `winget` instead.
#[cfg(target_os = "windows")]
fn fetch_pending_updates() -> Option<usize> {
    let out = std::process::Command::new("winget").arg("upgrade").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut count = 0usize;
    let mut past_header = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() && trimmed.chars().all(|c| c == '-') {
            past_header = true;
            continue;
        }
        if past_header && !trimmed.is_empty() {
            count += 1;
        }
    }
    Some(count)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn fetch_pending_updates() -> Option<usize> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_handle_reports_not_ready() {
        let handle = Tier3Handle::default();
        assert!(!handle.snapshot().ready);
    }

    #[test]
    fn bounded_fetch_never_waits_past_its_timeout() {
        // A 1ms budget is far shorter than any real network/package-manager
        // call can complete in, so this exercises the "network unreachable /
        // offline machine" path deterministically: the bound must hold
        // regardless of whether the spawned fetch ever finishes.
        let start = Instant::now();
        let snapshot = fetch_tier3_bounded(Duration::from_millis(1));
        let elapsed = start.elapsed();
        assert!(!snapshot.ready, "expected a timed-out snapshot, got: {snapshot:?}");
        assert!(snapshot.public_ip.is_none());
        assert!(snapshot.pending_updates.is_none());
        assert!(elapsed < Duration::from_millis(500), "bounded fetch took too long: {elapsed:?}");
    }

    #[test]
    fn set_then_snapshot_round_trips() {
        let handle = Tier3Handle::default();
        handle.set(Tier3Snapshot { public_ip: Some("203.0.113.7".to_string()), ready: true, ..Default::default() });
        let snap = handle.snapshot();
        assert!(snap.ready);
        assert_eq!(snap.public_ip.as_deref(), Some("203.0.113.7"));
    }
}
