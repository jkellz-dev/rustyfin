//! Logs and traces.
//!
//! Logs go to stdout, as text or as JSON lines (`[tracing] json_logs`). With
//! an OTLP endpoint (`[tracing] otlp_endpoint`), spans also go to an
//! OpenTelemetry collector, and each JSON line carries the id of the trace
//! it was written in, so a log line leads to its trace. Each SQL statement a
//! request runs is an event on its span, with the time it took; a scan's
//! statements are left out of its trace (tens of thousands per scan).

use std::io::Write;
use std::sync::OnceLock;

use opentelemetry::trace::{TraceContextExt, TracerProvider as _};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing::dispatcher::WeakDispatch;
use tracing::field::{Field, Visit};
use tracing::{Dispatch, Event, Subscriber};
use tracing_subscriber::filter::{EnvFilter, FilterExt, dynamic_filter_fn};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
use tracing_subscriber::registry::{LookupSpan, Registry};
use tracing_subscriber::util::SubscriberInitExt;

/// The log filter when `RUST_LOG` is unset.
pub const DEFAULT_LOG_FILTER: &str = "rustyfin=debug,tower_http=debug";

/// What goes into traces: Rustyfin's spans and events, and each SQL
/// statement (sqlx's `sqlx::query` events, which carry its time).
pub const TRACE_FILTER: &str = "rustyfin=debug,sqlx::query=debug";

/// The name of a library scan's span. SQL statements under it are not
/// traced.
pub const SCAN_SPAN: &str = "scan";

/// Set up logging, and traces when `otlp_endpoint` is given (an OTLP/HTTP
/// collector's base address, e.g. `http://collector:4318`). Returns the
/// tracer provider, to flush at shutdown.
pub fn init(
    otlp_endpoint: Option<&str>,
    json_logs: bool,
) -> anyhow::Result<Option<SdkTracerProvider>> {
    let provider = match otlp_endpoint.map(str::trim).filter(|e| !e.is_empty()) {
        Some(endpoint) => {
            let exporter = opentelemetry_otlp::SpanExporter::builder()
                .with_http()
                .with_endpoint(format!("{}/v1/traces", endpoint.trim_end_matches('/')))
                .build()?;
            let resource = opentelemetry_sdk::Resource::builder()
                .with_service_name("rustyfin")
                .build();
            Some(
                SdkTracerProvider::builder()
                    .with_batch_exporter(exporter)
                    .with_resource(resource)
                    .build(),
            )
        }
        None => None,
    };
    let log_filter = std::env::var("RUST_LOG").unwrap_or_else(|_| DEFAULT_LOG_FILTER.to_string());
    subscriber(provider.as_ref(), json_logs, std::io::stdout, &log_filter).try_init()?;
    if let Some(endpoint) = otlp_endpoint {
        tracing::info!("Sending traces to {endpoint}");
    }
    Ok(provider)
}

/// The subscriber `init` installs, writing logs to `writer`: text, or JSON
/// lines when `json_logs`; and spans to `provider`'s tracer, if any.
pub fn subscriber<W>(
    provider: Option<&SdkTracerProvider>,
    json_logs: bool,
    writer: W,
    log_filter: &str,
) -> impl Subscriber + Send + Sync + for<'a> LookupSpan<'a> + use<W>
where
    W: for<'w> MakeWriter<'w> + Clone + Send + Sync + 'static,
{
    // Both log layers are always there; the one not in use is filtered off.
    let (text_filter, json_filter) = if json_logs {
        (EnvFilter::new("off"), EnvFilter::new(log_filter))
    } else {
        (EnvFilter::new(log_filter), EnvFilter::new("off"))
    };
    let traces = provider.map(|provider| {
        // A scan's SQL statements stay out of its trace.
        let not_scan_sql = dynamic_filter_fn(|meta, cx: &Context<'_, Registry>| {
            if !meta.is_event() || !meta.target().starts_with("sqlx::query") {
                return true;
            }
            cx.lookup_current()
                .is_none_or(|span| !span.scope().any(|s| s.name() == SCAN_SPAN))
        });
        tracing_opentelemetry::layer()
            .with_tracer(provider.tracer("rustyfin"))
            .with_filter(EnvFilter::new(TRACE_FILTER).and(not_scan_sql))
    });
    Registry::default()
        .with(traces)
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(writer.clone())
                .with_filter(text_filter),
        )
        .with(JsonLines::new(writer).with_filter(json_filter))
}

/// One JSON object per log line: `timestamp` (UTC), `level`, `target`,
/// `message` and the event's other fields, `span` (the innermost one) and,
/// when traced, `trace_id` and `span_id`.
struct JsonLines<W> {
    writer: W,
    /// The subscriber this layer is part of, to read span contexts.
    dispatch: OnceLock<WeakDispatch>,
}

impl<W> JsonLines<W> {
    fn new(writer: W) -> Self {
        Self {
            writer,
            dispatch: OnceLock::new(),
        }
    }
}

impl<S, W> Layer<S> for JsonLines<W>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    W: for<'w> MakeWriter<'w> + 'static,
{
    fn on_register_dispatch(&self, subscriber: &Dispatch) {
        let _ = self.dispatch.set(subscriber.downgrade());
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let meta = event.metadata();
        let mut line = serde_json::Map::new();
        line.insert(
            "timestamp".into(),
            chrono::Utc::now()
                .to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
                .into(),
        );
        line.insert("level".into(), meta.level().as_str().into());
        line.insert("target".into(), meta.target().into());
        event.record(&mut JsonFields(&mut line));
        if let Some(scope) = ctx.event_scope(event) {
            let dispatch = self.dispatch.get().and_then(WeakDispatch::upgrade);
            let mut named = false;
            for span in scope {
                if !named {
                    line.insert("span".into(), span.name().into());
                    named = true;
                }
                // The innermost span with a trace (spans filtered out of
                // traces have none).
                let Some(dispatch) = &dispatch else { break };
                if let Some(cx) = tracing_opentelemetry::get_otel_context(&span.id(), dispatch) {
                    let span_ref = cx.span();
                    let sc = span_ref.span_context();
                    if sc.is_valid() {
                        line.insert("trace_id".into(), sc.trace_id().to_string().into());
                        line.insert("span_id".into(), sc.span_id().to_string().into());
                        break;
                    }
                }
            }
        }
        let Ok(mut text) = serde_json::to_string(&line) else {
            return;
        };
        text.push('\n');
        let _ = self.writer.make_writer_for(meta).write_all(text.as_bytes());
    }
}

/// An event's fields as JSON values (`message` among them).
struct JsonFields<'a>(&'a mut serde_json::Map<String, serde_json::Value>);

impl JsonFields<'_> {
    fn put(&mut self, field: &Field, value: serde_json::Value) {
        // tracing-log's bookkeeping fields, when a `log` record is bridged.
        if !field.name().starts_with("log.") {
            self.0.insert(field.name().to_string(), value);
        }
    }
}

impl Visit for JsonFields<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.put(field, format!("{value:?}").into());
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        self.put(field, value.into());
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.put(field, value.into());
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.put(field, value.into());
    }
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.put(field, value.into());
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.put(field, value.into());
    }
    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        self.put(field, value.to_string().into());
    }
}
