use std::sync::Arc;

use anyhow::Result;
use tokio::net::TcpListener;
use tokio::signal;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use ara_notification_service::config::Settings;
use ara_notification_service::server::{create_app, AppState};
use ara_notification_service::tasks::HeartbeatTask;
use ara_notification_service::triggers::RedisSubscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    init_tracing();

    // Load configuration
    let settings = Settings::new()?;
    tracing::info!("Configuration loaded");

    // Create application state
    let state = AppState::new(settings.clone());
    tracing::info!("Application state initialized");

    // Create Redis subscriber
    let redis_subscriber = Arc::new(RedisSubscriber::new(
        settings.redis.clone(),
        state.dispatcher.clone(),
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
        shutdown_signal.subscribe(),
    );
    let heartbeat_handle = tokio::spawn(async move {
        heartbeat_task.run().await;
    });

    // Create Axum app
    let app = create_app(state);

    // Start server
    let addr = settings.server_addr();
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("Server listening on {}", addr);

    // Run server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal_handler(shutdown_signal))
        .await?;

    // Wait for background tasks to finish
    tracing::info!("Waiting for background tasks to finish...");
    let _ = tokio::join!(redis_handle, heartbeat_handle);

    tracing::info!("Server shutdown complete");
    Ok(())
}

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
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
