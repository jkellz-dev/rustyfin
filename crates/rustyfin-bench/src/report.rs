//! What a run measured, as TOML (the results file) and as Markdown (stdout).

use std::fmt::Write;

use serde::Serialize;

use crate::machine::{Machine, round1};

#[derive(Debug, Serialize)]
pub struct Results {
    pub date: String,
    pub commit: Option<String>,
    pub scale: f64,
    pub files: u64,
    pub idle_memory_mib: Option<f64>,
    pub browse_memory_mib: Option<f64>,
    pub peak_memory_mib: Option<f64>,
    pub machine: Machine,
    pub first_scan: Scan,
    pub rescan: Scan,
    pub browse: Vec<Browse>,
}

#[derive(Debug, Serialize)]
pub struct Scan {
    pub seconds: f64,
    /// Server CPU time over the scan divided by its length.
    pub server_cores: Option<f64>,
    pub peak_memory_mib: Option<f64>,
    pub errors: u64,
    pub items: Items,
}

#[derive(Debug, Serialize, Clone, Copy)]
pub struct Items {
    pub films: u64,
    pub shows: u64,
    pub episodes: u64,
    pub albums: u64,
    pub tracks: u64,
    pub total: u64,
}

#[derive(Debug, Serialize)]
pub struct Browse {
    pub query: String,
    pub url: String,
    pub clients: usize,
    pub requests: usize,
    pub errors: usize,
    pub per_second: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub server_cores: Option<f64>,
}

fn opt(v: Option<f64>, unit: &str) -> String {
    v.map_or_else(|| "n/a".to_string(), |v| format!("{} {unit}", round1(v)))
}

fn duration(secs: f64) -> String {
    if secs >= 120.0 {
        format!("{:.1} min", secs / 60.0)
    } else {
        format!("{secs:.1} s")
    }
}

impl Results {
    pub fn markdown(&self) -> String {
        let mut s = String::new();
        let m = &self.machine;
        let _ = writeln!(
            s,
            "Rustyfin {} on {} ({} CPUs, {}), {}, scale {} ({} files)",
            self.commit.as_deref().unwrap_or("(unknown commit)"),
            m.cpu,
            m.cpus,
            opt(m.memory_gib, "GiB"),
            m.os,
            self.scale,
            self.files
        );
        let _ = writeln!(
            s,
            "Postgres {}, ffprobe {}\n",
            m.postgres.as_deref().unwrap_or("?"),
            m.ffprobe.as_deref().unwrap_or("?")
        );
        let i = &self.first_scan.items;
        let _ = writeln!(
            s,
            "Items: {} films, {} shows, {} episodes, {} albums, {} tracks; {} in all.\n",
            i.films, i.shows, i.episodes, i.albums, i.tracks, i.total
        );
        let _ = writeln!(s, "| Scan | Time | Server CPU | Peak memory | Errors |");
        let _ = writeln!(s, "| --- | --- | --- | --- | --- |");
        for (name, scan) in [("First scan", &self.first_scan), ("Rescan", &self.rescan)] {
            let _ = writeln!(
                s,
                "| {name} | {} | {} | {} | {} |",
                duration(scan.seconds),
                opt(scan.server_cores, "cores"),
                opt(scan.peak_memory_mib, "MiB"),
                scan.errors
            );
        }
        let clients = self.browse.first().map_or(0, |b| b.clients);
        let _ = writeln!(s, "\nBrowsing, {clients} concurrent clients per query:\n");
        let _ = writeln!(s, "| Query | Requests/s | p50 | p95 | p99 | Errors |");
        let _ = writeln!(s, "| --- | --- | --- | --- | --- | --- |");
        for b in &self.browse {
            let _ = writeln!(
                s,
                "| {} | {:.0} | {:.0} ms | {:.0} ms | {:.0} ms | {} |",
                b.query, b.per_second, b.p50_ms, b.p95_ms, b.p99_ms, b.errors
            );
        }
        let _ = writeln!(
            s,
            "\nServer memory: idle {}, browsing {}, peak {}.",
            opt(self.idle_memory_mib, "MiB"),
            opt(self.browse_memory_mib, "MiB"),
            opt(self.peak_memory_mib, "MiB")
        );
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Results {
        let items = Items {
            films: 270,
            shows: 30,
            episodes: 315,
            albums: 750,
            tracks: 7500,
            total: 9000,
        };
        let scan = |seconds| Scan {
            seconds,
            server_cores: Some(1.25),
            peak_memory_mib: Some(120.0),
            errors: 0,
            items,
        };
        Results {
            date: "2026-11-17T00:00:00Z".into(),
            commit: Some("abc1234".into()),
            scale: 0.1,
            files: 8139,
            idle_memory_mib: Some(80.0),
            browse_memory_mib: Some(90.0),
            peak_memory_mib: Some(130.0),
            machine: Machine {
                cpu: "Test CPU".into(),
                cpus: 4,
                memory_gib: Some(16.0),
                os: "linux x86_64".into(),
                postgres: Some("17.2".into()),
                ffprobe: Some("7.0.2".into()),
            },
            first_scan: scan(372.0),
            rescan: scan(4.2),
            search: None,
            browse: vec![Browse {
                query: "7500 tracks, first page".into(),
                url: "/Items?IncludeItemTypes=Audio".into(),
                clients: 5,
                requests: 1000,
                errors: 0,
                per_second: 33.3,
                p50_ms: 12.4,
                p95_ms: 20.6,
                p99_ms: 31.0,
                server_cores: Some(0.5),
            }],
        }
    }

    #[test]
    fn markdown_names_the_machine_and_the_numbers() {
        let md = sample().markdown();
        assert!(
            md.contains("Rustyfin abc1234 on Test CPU (4 CPUs, 16 GiB)"),
            "{md}"
        );
        assert!(
            md.contains("| First scan | 6.2 min | 1.3 cores | 120 MiB | 0 |"),
            "{md}"
        );
        assert!(md.contains("| Rescan | 4.2 s |"), "{md}");
        assert!(
            md.contains("| 7500 tracks, first page | 33 | 12 ms | 21 ms | 31 ms | 0 |"),
            "{md}"
        );
    }

    fn with_meilisearch() -> Results {
        let index = |seconds| SearchIndex {
            seconds_after_scan: Some(seconds),
            documents: 9000,
            failed_tasks: 0,
        };
        Results {
            search: Some(Search {
                meilisearch: "1.57.0".into(),
                first_scan: index(12.5),
                rescan: index(3.0),
                peak_memory_mib: Some(210.0),
            }),
            ..sample()
        }
    }

    #[test]
    fn markdown_reports_the_search_index() {
        let md = with_meilisearch().markdown();
        assert!(md.contains("Postgres 17.2, ffprobe 7.0.2, Meilisearch 1.57.0"), "{md}");
        assert!(
            md.contains("| Meilisearch index after the first scan | 12.5 s | 9000 of 9000 items | 0 failed |"),
            "{md}"
        );
        assert!(
            md.contains("| Meilisearch index after the rescan | 3.0 s | 9000 of 9000 items | 0 failed |"),
            "{md}"
        );
        assert!(md.contains("Meilisearch memory: peak 210 MiB."), "{md}");
    }

    #[test]
    fn an_index_that_never_settles_says_so() {
        let mut r = with_meilisearch();
        if let Some(search) = r.search.as_mut() {
            search.first_scan.seconds_after_scan = None;
            search.first_scan.documents = 100;
        }
        let md = r.markdown();
        assert!(
            md.contains("| Meilisearch index after the first scan | did not settle | 100 of 9000 items |"),
            "{md}"
        );
    }

    #[test]
    fn without_meilisearch_the_report_says_postgres_only() {
        let md = sample().markdown();
        assert!(md.contains("Search: Postgres only (no Meilisearch)"), "{md}");
        assert!(!md.contains("Meilisearch index"), "{md}");
    }

    #[test]
    fn search_results_are_toml() {
        let text = toml::to_string(&with_meilisearch()).unwrap();
        assert!(text.contains("[search.first_scan]"), "{text}");
        assert!(text.contains("meilisearch = \"1.57.0\""), "{text}");
        assert!(!toml::to_string(&sample()).unwrap().contains("[search"));
    }

    #[test]
    fn results_are_toml() {
        let text = toml::to_string(&sample()).unwrap();
        assert!(text.contains("scale = 0.1"), "{text}");
        assert!(text.contains("[first_scan.items]"), "{text}");
        assert!(text.contains("[[browse]]"), "{text}");
    }
}
