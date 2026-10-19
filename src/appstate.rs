use std::sync::Arc;

use courseviewer::courseeraclient;

pub struct AppState {
    pub client: Arc<courseeraclient::CourseEraClient>,
}

impl AppState {
    pub fn new(client: Arc<courseeraclient::CourseEraClient>) -> Self {
        Self { client }
    }
}
