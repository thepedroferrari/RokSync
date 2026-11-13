//! Finds Hue bridges on the LAN via mDNS (`_hue._tcp`).

use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub ip: String,
    pub source: &'static str,
}

pub fn discover(timeout: Duration) -> Vec<Found> {
    let mut out: Vec<Found> = Vec::new();
    for ip in mdns(timeout).unwrap_or_default() {
        if !out.iter().any(|f| f.ip == ip) {
            out.push(Found { ip, source: "mDNS" });
        }
    }
    if let Some(ip) = huesync_hint() {
        if !out.iter().any(|f| f.ip == ip) {
            out.push(Found { ip, source: "Hue Sync config" });
        }
    }
    out
}

fn mdns(timeout: Duration) -> anyhow::Result<Vec<String>> {
    let daemon = mdns_sd::ServiceDaemon::new()?;
    let rx = daemon.browse("_hue._tcp.local.")?;
    let deadline = Instant::now() + timeout;
    let mut out = Vec::new();
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(left) {
            Ok(mdns_sd::ServiceEvent::ServiceResolved(info)) => {
                for a in info.get_addresses() {
                    let s = a.to_string();
                    if !s.contains(':') && !out.contains(&s) {
                        out.push(s);
                    }
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    let _ = daemon.shutdown();
    Ok(out)
}

/// The IP Philips' own Hue Sync app last used, if it's installed.
pub fn huesync_hint() -> Option<String> {
    let path = dirs::config_dir()?.join("HueSync").join("bridge.json");
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    v.get("Bridges")?.get(0)?.get("IpAddress")?.as_str().map(str::to_string)
}
