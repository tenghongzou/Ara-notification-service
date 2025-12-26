use std::sync::Arc;

use crate::auth::JwtValidator;
use crate::config::Settings;
use crate::connection_manager::ConnectionManager;
use crate::notification::NotificationDispatcher;

#[derive(Clone)]
pub struct AppState {
    pub settings: Arc<Settings>,
    pub jwt_validator: Arc<JwtValidator>,
    pub connection_manager: Arc<ConnectionManager>,
    pub dispatcher: Arc<NotificationDispatcher>,
}

impl AppState {
    pub fn new(settings: Settings) -> Self {
        let jwt_validator = Arc::new(JwtValidator::new(&settings.jwt));
        let connection_manager = Arc::new(ConnectionManager::new());
        let dispatcher = Arc::new(NotificationDispatcher::new(connection_manager.clone()));

        Self {
            settings: Arc::new(settings),
            jwt_validator,
            connection_manager,
            dispatcher,
        }
    }
}
