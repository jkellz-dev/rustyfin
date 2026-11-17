//! The machine a run happened on, and the server process's CPU and memory
//! (Linux `/proc`; elsewhere these are left out of the report).

use std::path::Path;

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Machine {
    pub cpu: String,
    /// CPUs this process may use (cgroup limits included).
    pub cpus: usize,
    /// The memory this process may use: its cgroup's limit, else the machine's.
    pub memory_gib: Option<f64>,
    pub os: String,
    pub postgres: Option<String>,
    pub ffprobe: Option<String>,
}

pub fn describe(postgres: Option<String>) -> Machine {
    let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let cpu = cpuinfo
        .lines()
        .find_map(|l| l.strip_prefix("model name").and_then(|r| r.split_once(':')))
        .map(|(_, v)| v.trim().to_string())
        .unwrap_or_else(|| std::env::consts::ARCH.to_string());
    let machine_kib = meminfo_kib("MemTotal");
    let limit_kib = std::fs::read_to_string("/sys/fs/cgroup/memory.max")
        .ok()
        .and_then(|s| s.trim().parse::<f64>().ok())
        .map(|bytes| bytes / 1024.0);
    let kib = match (limit_kib, machine_kib) {
        (Some(l), Some(m)) => Some(l.min(m)),
        (l, m) => l.or(m),
    };
    Machine {
        cpu,
        cpus: std::thread::available_parallelism().map_or(1, |n| n.get()),
        memory_gib: kib.map(|k| round1(k / 1024.0 / 1024.0)),
        os: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        postgres,
        ffprobe: ffprobe_version(),
    }
}

/// `ffprobe version 7.0.2-static ...` -> `7.0.2-static`.
fn ffprobe_version() -> Option<String> {
    let out = std::process::Command::new("ffprobe")
        .arg("-version")
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let first = text.lines().next()?;
    first
        .strip_prefix("ffprobe version ")?
        .split_whitespace()
        .next()
        .map(str::to_string)
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

/// CPU time the process and its finished children (the ffprobe runs of a
/// scan) have used, user plus system, in seconds.
/// Every process below `pid` (its ffmpeg processes, say), from `/proc`.
fn descendants(pid: u32) -> Vec<u32> {
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    let parents: Vec<(u32, u32)> = dir
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().to_str()?.parse::<u32>().ok())
        .filter_map(|p| {
            let stat = std::fs::read_to_string(format!("/proc/{p}/stat")).ok()?;
            let rest = &stat[stat.rfind(')')? + 2..];
            let ppid = rest.split_whitespace().nth(1)?.parse().ok()?;
            Some((p, ppid))
        })
        .collect();
    let mut out = Vec::new();
    let mut todo = vec![pid];
    while let Some(parent) = todo.pop() {
        for &(p, pp) in &parents {
            if pp == parent {
                out.push(p);
                todo.push(p);
            }
        }
    }
    out
}

/// CPU seconds of `pid` and every process below it, running or finished.
pub fn tree_cpu_seconds(pid: u32) -> Option<f64> {
    let own = cpu_seconds(pid)?;
    Some(
        own + descendants(pid)
            .into_iter()
            .filter_map(cpu_seconds)
            .sum::<f64>(),
    )
}

/// Resident memory of `pid` and every process below it, together.
pub fn tree_memory_mib(pid: u32) -> Option<f64> {
    let own = memory_mib(pid)?;
    let total = own
        + descendants(pid)
            .into_iter()
            .filter_map(memory_mib)
            .sum::<f64>();
    Some(round1(total))
}

pub fn cpu_seconds(pid: u32) -> Option<f64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // Fields after the command name, which is in parentheses and may hold spaces.
    let rest = &stat[stat.rfind(')')? + 2..];
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // utime, stime, cutime and cstime are fields 14 to 17 of the line, 12 to 15 here.
    let mut ticks = 0.0;
    for i in 11..=14 {
        ticks += fields.get(i)?.parse::<f64>().ok()?;
    }
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
        let m = describe(None);
        assert!(m.cpus >= 1);
        assert!(!m.cpu.is_empty());
    }
}
