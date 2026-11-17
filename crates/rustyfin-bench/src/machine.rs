//! The machine a run happened on, and the server process's CPU and memory
//! (Linux `/proc`; elsewhere these are left out of the report).

use std::path::Path;

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Machine {
    pub cpu: String,
    /// CPUs this process may use (cgroup limits included).
    pub cpus: usize,
    pub memory_gib: Option<f64>,
    pub os: String,
}

pub fn describe() -> Machine {
    let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let cpu = cpuinfo
        .lines()
        .find_map(|l| l.strip_prefix("model name").and_then(|r| r.split_once(':')))
        .map(|(_, v)| v.trim().to_string())
        .unwrap_or_else(|| std::env::consts::ARCH.to_string());
    let memory_gib = meminfo_kib("MemTotal").map(|k| round1(k / 1024.0 / 1024.0));
    Machine {
        cpu,
        cpus: std::thread::available_parallelism().map_or(1, |n| n.get()),
        memory_gib,
        os: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
    }
}

fn meminfo_kib(key: &str) -> Option<f64> {
    let text = std::fs::read_to_string("/proc/meminfo").ok()?;
    text.lines()
        .find_map(|l| l.strip_prefix(key)?.strip_prefix(':'))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

/// A field of `/proc/<pid>/status` in MiB (`VmRSS`, `VmHWM`).
fn status_mib(pid: u32, key: &str) -> Option<f64> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let kib: f64 = text
        .lines()
        .find_map(|l| l.strip_prefix(key)?.strip_prefix(':'))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    Some(round1(kib / 1024.0))
}

/// Resident memory now.
pub fn memory_mib(pid: u32) -> Option<f64> {
    status_mib(pid, "VmRSS")
}

/// The most resident memory so far.
pub fn peak_memory_mib(pid: u32) -> Option<f64> {
    status_mib(pid, "VmHWM")
}

/// User plus system CPU time the process has used, in seconds.
pub fn cpu_seconds(pid: u32) -> Option<f64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // Fields after the command name, which is in parentheses and may hold spaces.
    let rest = &stat[stat.rfind(')')? + 2..];
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // utime and stime are fields 14 and 15 of the line, 12 and 13 here.
    let ticks: f64 = fields.get(11)?.parse::<f64>().ok()? + fields.get(12)?.parse::<f64>().ok()?;
    Some(ticks / clock_ticks())
}

/// `sysconf(_SC_CLK_TCK)`: 100 on every Linux in practice.
fn clock_ticks() -> f64 {
    100.0
}

pub fn git_commit(root: &Path) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(root)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_this_process() {
        if !Path::new("/proc/self/status").exists() {
            return;
        }
        let me = std::process::id();
        assert!(memory_mib(me).is_some_and(|m| m > 0.0));
        assert!(peak_memory_mib(me).is_some_and(|m| m > 0.0));
        assert!(cpu_seconds(me).is_some_and(|s| s >= 0.0));
    }

    #[test]
    fn describes_the_machine() {
        let m = describe();
        assert!(m.cpus >= 1);
        assert!(!m.cpu.is_empty());
    }
}
