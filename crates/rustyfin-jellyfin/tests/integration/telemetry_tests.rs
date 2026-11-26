//! Owner, 2026-11-24: traces with Tempo and logs with Loki, "to get all the
//! timings with really fine grained fidelity". Spans go to an OpenTelemetry
//! collector; each JSON log line carries its trace's id, so Grafana leads
//! from a log line to its trace. A scan's SQL statements stay out of its
//! trace (tens of thousands each scan); a request's are kept, with their
//! times.
use std::io::Write;
use std::sync::{Arc, Mutex};

use opentelemetry_sdk::trace::{InMemorySpanExporter, SdkTracerProvider};
use rustyfin_jellyfin::telemetry;

/// Log output captured in memory.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Captured {
    fn lines(&self) -> Vec<serde_json::Value> {
        String::from_utf8(self.0.lock().unwrap().clone())
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("{e}: {l}")))
            .collect()
    }
}

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
    type Writer = Captured;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

fn traced() -> (InMemorySpanExporter, SdkTracerProvider) {
    let exporter = InMemorySpanExporter::default();
    let provider = SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    (exporter, provider)
}

#[test]
fn json_log_lines_carry_their_trace_id() {
    let (exporter, provider) = traced();
    let out = Captured::default();
    let subscriber = telemetry::subscriber(Some(&provider), true, out.clone(), "rustyfin=debug");
    tracing::subscriber::with_default(subscriber, || {
        let span =
            tracing::info_span!(target: "rustyfin_jellyfin::albums", "album", name = "Wet Tennis");
        span.in_scope(|| {
            tracing::info!(target: "rustyfin_jellyfin::albums", tracks = 2, "read the album");
        });
        // Outside any span: no trace.
        tracing::warn!(target: "rustyfin_jellyfin::albums", "no span here");
        // Below the log filter: not written.
        tracing::trace!(target: "rustyfin_jellyfin::albums", "too quiet");
    });
    provider.force_flush().unwrap();
    let spans = exporter.get_finished_spans().unwrap();
    assert_eq!(spans.len(), 1, "{spans:?}");
    assert_eq!(spans[0].name, "album");

    let lines = out.lines();
    assert_eq!(lines.len(), 2, "{lines:?}");
    let first = &lines[0];
    assert_eq!(first["message"], "read the album", "{first}");
    assert_eq!(first["level"], "INFO", "{first}");
    assert_eq!(first["target"], "rustyfin_jellyfin::albums", "{first}");
    assert_eq!(first["tracks"], 2, "{first}");
    assert_eq!(first["span"], "album", "{first}");
    assert_eq!(
        first["trace_id"],
        spans[0].span_context.trace_id().to_string(),
        "{first}"
    );
    assert_eq!(
        first["span_id"],
        spans[0].span_context.span_id().to_string(),
        "{first}"
    );
    assert!(
        first["timestamp"]
            .as_str()
            .is_some_and(|t| t.ends_with('Z')),
        "{first}"
    );
    assert_eq!(lines[1]["message"], "no span here");
    assert_eq!(lines[1]["level"], "WARN");
    assert!(lines[1].get("trace_id").is_none(), "{}", lines[1]);
}

#[test]
fn without_a_collector_json_lines_have_no_trace() {
    let out = Captured::default();
    let subscriber = telemetry::subscriber(None, true, out.clone(), "rustyfin=debug");
    tracing::subscriber::with_default(subscriber, || {
        let span = tracing::info_span!(target: "rustyfin_jellyfin::albums", "album");
        span.in_scope(|| tracing::info!(target: "rustyfin_jellyfin::albums", "read the album"));
    });
    let lines = out.lines();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0]["span"], "album");
    assert!(lines[0].get("trace_id").is_none(), "{}", lines[0]);
}

#[test]
fn a_scans_sql_stays_out_of_its_trace_and_a_requests_is_kept() {
    let (exporter, provider) = traced();
    let subscriber = telemetry::subscriber(Some(&provider), true, Captured::default(), "off");
    tracing::subscriber::with_default(subscriber, || {
        let request = tracing::info_span!(target: "rustyfin_jellyfin::metrics", "http.request");
        request.in_scope(|| {
            tracing::debug!(target: "sqlx::query", summary = "select name", elapsed_secs = 0.002, "select name from items");
        });
        let scan = tracing::info_span!(target: "rustyfin_services::library::scanner", telemetry::SCAN_SPAN);
        scan.in_scope(|| {
            let album = tracing::info_span!(target: "rustyfin_services::library::scanner", "album");
            album.in_scope(|| {
                tracing::debug!(target: "sqlx::query", summary = "select etag", "select etag from items");
                // Rustyfin's own events are kept in a scan.
                tracing::info!(target: "rustyfin_services::library::scanner", "album changed");
            });
        });
    });
    provider.force_flush().unwrap();
    let spans = exporter.get_finished_spans().unwrap();
    let events = |name: &str| -> Vec<String> {
        spans
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("no {name} in {spans:?}"))
            .events
            .events
            .iter()
            .map(|e| e.name.to_string())
            .collect()
    };
    assert_eq!(events("http.request"), ["select name from items"]);
    assert_eq!(events("album"), ["album changed"]);
    assert!(
        spans.iter().any(|s| s.name == telemetry::SCAN_SPAN),
        "{spans:?}"
    );
}
