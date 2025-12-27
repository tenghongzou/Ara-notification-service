//! OpenTelemetry telemetry module for distributed tracing.
//!
//! This module provides:
//! - OTLP exporter configuration for sending traces to collectors like Jaeger, Zipkin, or Tempo
//! - Integration with the `tracing` crate for seamless span creation
//! - Configurable sampling for production environments
//!
//! # Environment Variables
//!
//! | Variable | Description | Default |
//! |----------|-------------|---------|
//! | `OTEL_ENABLED` | Enable OpenTelemetry tracing | `false` |
//! | `OTEL_ENDPOINT` | OTLP gRPC endpoint | `http://localhost:4317` |
//! | `OTEL_SERVICE_NAME` | Service name in traces | `ara-notification-service` |
//! | `OTEL_SAMPLING_RATIO` | Trace sampling ratio (0.0-1.0) | `1.0` |

use opentelemetry::trace::TracerProvider;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    runtime,
    trace::{RandomIdGenerator, Sampler, TracerProvider as SdkTracerProvider},
    Resource,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::config::OtelConfig;

/// Result type for telemetry operations
pub type TelemetryResult<T> = Result<T, TelemetryError>;

/// Telemetry-specific error type
#[derive(Debug, thiserror::Error)]
pub enum TelemetryError {
    #[error("Failed to initialize OpenTelemetry tracer: {0}")]
    TracerInit(String),
    #[error("Failed to build OTLP exporter: {0}")]
    ExporterBuild(String),
}

/// Telemetry guard that ensures proper shutdown of OpenTelemetry on drop.
pub struct TelemetryGuard {
    _provider: Option<SdkTracerProvider>,
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        if self._provider.is_some() {
            // Shutdown is handled automatically by TracerProvider drop
            tracing::info!("Shutting down OpenTelemetry tracer provider");
        }
    }
}

/// Initialize the telemetry system with the given configuration.
///
/// This function sets up the tracing subscriber with:
/// - Console output for local debugging
/// - OpenTelemetry layer for distributed tracing (if enabled)
///
/// # Arguments
///
/// * `config` - OpenTelemetry configuration
///
/// # Returns
///
/// A `TelemetryGuard` that should be kept alive for the duration of the application.
/// When dropped, it ensures proper shutdown of the OpenTelemetry tracer.
pub fn init_telemetry(config: &OtelConfig) -> TelemetryResult<TelemetryGuard> {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    if config.enabled {
        // Initialize OpenTelemetry with OTLP exporter
        let provider = init_otel_tracer(config)?;
        let tracer = provider.tracer("ara-notification-service");
        let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer())
            .with(otel_layer)
            .init();

        tracing::info!(
            endpoint = %config.endpoint,
            service_name = %config.service_name,
            sampling_ratio = %config.sampling_ratio,
            "OpenTelemetry tracing initialized"
        );

        Ok(TelemetryGuard {
            _provider: Some(provider),
        })
    } else {
        // Standard logging without OpenTelemetry
        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer())
            .init();

        tracing::info!("Tracing initialized (OpenTelemetry disabled)");

        Ok(TelemetryGuard { _provider: None })
    }
}

/// Initialize the OpenTelemetry tracer with OTLP exporter.
fn init_otel_tracer(config: &OtelConfig) -> TelemetryResult<SdkTracerProvider> {
    use opentelemetry::KeyValue;

    // Create OTLP exporter
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&config.endpoint)
        .build()
        .map_err(|e| TelemetryError::ExporterBuild(e.to_string()))?;

    // Configure sampler based on sampling ratio
    let sampler = if config.sampling_ratio >= 1.0 {
        Sampler::AlwaysOn
    } else if config.sampling_ratio <= 0.0 {
        Sampler::AlwaysOff
    } else {
        Sampler::TraceIdRatioBased(config.sampling_ratio)
    };

    // Build the tracer provider
    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter, runtime::Tokio)
        .with_sampler(sampler)
        .with_id_generator(RandomIdGenerator::default())
        .with_resource(Resource::new(vec![
            KeyValue::new(
                opentelemetry_semantic_conventions::resource::SERVICE_NAME,
                config.service_name.clone(),
            ),
            KeyValue::new(
                opentelemetry_semantic_conventions::resource::SERVICE_VERSION,
                env!("CARGO_PKG_VERSION"),
            ),
        ]))
        .build();

    Ok(provider)
}

/// Utility module for creating common span attributes.
pub mod attributes {
    use opentelemetry::KeyValue;

    /// Create a KeyValue for user ID.
    pub fn user_id(id: &str) -> KeyValue {
        KeyValue::new("user.id", id.to_string())
    }

    /// Create a KeyValue for connection ID.
    pub fn connection_id(id: uuid::Uuid) -> KeyValue {
        KeyValue::new("connection.id", id.to_string())
    }

    /// Create a KeyValue for notification ID.
    pub fn notification_id(id: uuid::Uuid) -> KeyValue {
        KeyValue::new("notification.id", id.to_string())
    }

    /// Create a KeyValue for target type.
    pub fn target_type(t: &str) -> KeyValue {
        KeyValue::new("notification.target_type", t.to_string())
    }

    /// Create a KeyValue for channel name.
    pub fn channel(name: &str) -> KeyValue {
        KeyValue::new("channel.name", name.to_string())
    }

    /// Create a KeyValue for event type.
    pub fn event_type(t: &str) -> KeyValue {
        KeyValue::new("notification.event_type", t.to_string())
    }

    /// Create a KeyValue for delivery count.
    pub fn delivered_count(count: usize) -> KeyValue {
        KeyValue::new("notification.delivered_count", count as i64)
    }

    /// Create a KeyValue for failed count.
    pub fn failed_count(count: usize) -> KeyValue {
        KeyValue::new("notification.failed_count", count as i64)
    }

    /// Create a KeyValue for WebSocket message type.
    pub fn ws_message_type(t: &str) -> KeyValue {
        KeyValue::new("ws.message_type", t.to_string())
    }

    /// Create a KeyValue for HTTP method.
    pub fn http_method(method: &str) -> KeyValue {
        KeyValue::new("http.method", method.to_string())
    }

    /// Create a KeyValue for HTTP path.
    pub fn http_path(path: &str) -> KeyValue {
        KeyValue::new("http.path", path.to_string())
    }

    /// Create a KeyValue for HTTP status code.
    pub fn http_status(code: u16) -> KeyValue {
        KeyValue::new("http.status_code", code as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = OtelConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.endpoint, "http://localhost:4317");
        assert_eq!(config.service_name, "ara-notification-service");
        assert_eq!(config.sampling_ratio, 1.0);
    }

    #[test]
    fn test_attributes() {
        let user = attributes::user_id("user-123");
        assert_eq!(user.key.as_str(), "user.id");

        let conn = attributes::connection_id(uuid::Uuid::nil());
        assert_eq!(conn.key.as_str(), "connection.id");

        let notif = attributes::notification_id(uuid::Uuid::nil());
        assert_eq!(notif.key.as_str(), "notification.id");
    }

    #[test]
    fn test_telemetry_guard_creation() {
        let guard = TelemetryGuard { _provider: None };
        drop(guard); // Should not panic
    }
}
