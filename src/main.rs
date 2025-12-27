use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::net::TcpListener;
use tokio::signal;
use tokio::time::timeout;

use ara_notification_service::cluster::RoutedMessageSubscriber;
use ara_notification_service::config::Settings;
use ara_notification_service::server::{create_app, AppState};
use ara_notification_service::tasks::HeartbeatTask;
use ara_notification_service::telemetry::init_telemetry;
use ara_notification_service::triggers::RedisSubscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Load configuration first (needed for telemetry config)
    let settings = Settings::new()?;

    // Initialize telemetry (tracing + optional OpenTelemetry)
    let _telemetry_guard = init_telemetry(&settings.otel)
        .expect("Failed to initialize telemetry");

    tracing::info!("Configuration loaded");

    // Create application state
    let state = AppState::new(settings.clone()).await;
    tracing::info!("Application state initialized");

    // Create Redis subscriber with circuit breaker and health from state
    let redis_subscriber = Arc::new(RedisSubscriber::new(
        settings.redis.clone(),
        state.dispatcher.clone(),
        state.redis_circuit_breaker.clone(),
        state.redis_health.clone(),
    ));
    let shutdown_signal = redis_subscriber.shutdown_signal();

    // Start Redis subscriber in background
    let redis_subscriber_clone = redis_subscriber.clone();
    let redis_handle = tokio::spawn(async move {
        if let Err(e) = redis_subscriber_clone.start().await {
            tracing::error!(error = %e, "Redis subscriber failed");
        }
    });

    // Start heartbeat task in background
    let heartbeat_task = HeartbeatTask::new(
        settings.websocket.clone(),
        state.connection_manager.clone(),
        state.session_store.clone(),
        shutdown_signal.subscribe(),
    );
    let heartbeat_handle = tokio::spawn(async move {
        heartbeat_task.run().await;
    });

    // Start cluster routed message subscriber in background (if cluster mode is enabled and Redis is available)
    let cluster_handle = if settings.cluster.enabled {
        if let Some(ref redis_pool) = state.redis_pool {
            let subscriber = RoutedMessageSubscriber::new(
                settings.cluster.clone(),
                redis_pool.clone(),
                state.cluster_router.clone(),
                shutdown_signal.subscribe(),
            );
            Some(tokio::spawn(async move {
                subscriber.run().await;
            }))
        } else {
            tracing::warn!("Cluster mode enabled but Redis pool not available, skipping routed message subscriber");
            None
        }
    } else {
        None
    };

    // Create Axum app
    let app = create_app(state);

    // Start server
    let addr = settings.server_addr();
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("Server listening on {}", addr);

    // Run server with graceful shutdown
    // Use into_make_service_with_connect_info for rate limiting by IP
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal_handler(shutdown_signal))
    .await?;

    // Wait for background tasks to finish with timeout
    const SHUTDOWN_TIMEOUT_SECS: u64 = 30;
    tracing::info!(
        timeout_secs = SHUTDOWN_TIMEOUT_SECS,
        "Waiting for background tasks to finish..."
    );

    let shutdown_future = async {
        let _ = tokio::join!(redis_handle, heartbeat_handle);
        if let Some(handle) = cluster_handle {
            let _ = handle.await;
        }
    };

    match timeout(Duration::from_secs(SHUTDOWN_TIMEOUT_SECS), shutdown_future).await {
        Ok(_) => {
            tracing::info!("All background tasks completed gracefully");
        }
        Err(_) => {
            tracing::warn!(
                timeout_secs = SHUTDOWN_TIMEOUT_SECS,
                "Shutdown timeout exceeded, forcing exit"
            );
        }
    }

    tracing::info!("Server shutdown complete");
    Ok(())
}

async fn shutdown_signal_handler(shutdown_tx: tokio::sync::broadcast::Sender<()>) {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("Received Ctrl+C, initiating graceful shutdown");
        }
        _ = terminate => {
            tracing::info!("Received terminate signal, initiating graceful shutdown");
        }
    }

    // Send shutdown signal to Redis subscriber
    let _ = shutdown_tx.send(());
}
