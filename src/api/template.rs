//! Template CRUD endpoints.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;

use crate::server::AppState;
use crate::template::{
    CreateTemplateRequest, Template, TemplateError, TemplateListResponse, UpdateTemplateRequest,
};

#[derive(Debug, Serialize)]
pub struct TemplateErrorResponse {
    pub error: TemplateErrorInfo,
}

#[derive(Debug, Serialize)]
pub struct TemplateErrorInfo {
    pub code: String,
    pub message: String,
}

impl From<TemplateError> for (StatusCode, Json<TemplateErrorResponse>) {
    fn from(err: TemplateError) -> Self {
        let (status, code) = match &err {
            TemplateError::NotFound(_) => (StatusCode::NOT_FOUND, "TEMPLATE_NOT_FOUND"),
            TemplateError::AlreadyExists(_) => (StatusCode::CONFLICT, "TEMPLATE_EXISTS"),
            TemplateError::InvalidId(_) => (StatusCode::BAD_REQUEST, "INVALID_ID"),
            TemplateError::InvalidTemplate(_) => (StatusCode::BAD_REQUEST, "INVALID_TEMPLATE"),
            TemplateError::SubstitutionFailed(_) => {
                (StatusCode::UNPROCESSABLE_ENTITY, "SUBSTITUTION_FAILED")
            }
        };

        (
            status,
            Json(TemplateErrorResponse {
                error: TemplateErrorInfo {
                    code: code.to_string(),
                    message: err.to_string(),
                },
            }),
        )
    }
}

/// POST /api/v1/templates - Create a new template
#[tracing::instrument(
    name = "http.create_template",
    skip(state, request),
    fields(template_id = %request.id)
)]
pub async fn create_template(
    State(state): State<AppState>,
    Json(request): Json<CreateTemplateRequest>,
) -> Result<(StatusCode, Json<Template>), (StatusCode, Json<TemplateErrorResponse>)> {
    let template: Template = request.into();

    match state.template_store.create(template) {
        Ok(created) => Ok((StatusCode::CREATED, Json(created))),
        Err(e) => Err(e.into()),
    }
}

/// GET /api/v1/templates - List all templates
#[tracing::instrument(name = "http.list_templates", skip(state))]
pub async fn list_templates(State(state): State<AppState>) -> Json<TemplateListResponse> {
    let templates = state.template_store.list();
    let total = templates.len();

    Json(TemplateListResponse { templates, total })
}

/// GET /api/v1/templates/:id - Get a specific template
#[tracing::instrument(name = "http.get_template", skip(state))]
pub async fn get_template(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Template>, (StatusCode, Json<TemplateErrorResponse>)> {
    match state.template_store.get(&id) {
        Ok(template) => Ok(Json(template)),
        Err(e) => Err(e.into()),
    }
}

/// PUT /api/v1/templates/:id - Update an existing template
#[tracing::instrument(name = "http.update_template", skip(state, request))]
pub async fn update_template(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateTemplateRequest>,
) -> Result<Json<Template>, (StatusCode, Json<TemplateErrorResponse>)> {
    match state.template_store.update(&id, request) {
        Ok(updated) => Ok(Json(updated)),
        Err(e) => Err(e.into()),
    }
}

/// DELETE /api/v1/templates/:id - Delete a template
#[tracing::instrument(name = "http.delete_template", skip(state))]
pub async fn delete_template(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<TemplateErrorResponse>)> {
    match state.template_store.delete(&id) {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(e) => Err(e.into()),
    }
}
