//! Template CRUD endpoints.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Serialize;

use crate::server::middleware::RequestTenantContext;
use crate::server::AppState;
use crate::template::{
    CreateTemplateRequest, Template, TemplateError, TemplateListResponse, UpdateTemplateRequest,
};

/// Prefix a template ID with tenant scope for isolation
fn tenant_template_id(tenant_ctx: &Option<Extension<RequestTenantContext>>, id: &str) -> String {
    match tenant_ctx.as_ref() {
        Some(t) if !t.0 .0.is_default => format!("{}:{}", t.0.tenant_id(), id),
        _ => id.to_string(),
    }
}

/// Get tenant prefix for filtering templates
fn tenant_prefix(tenant_ctx: &Option<Extension<RequestTenantContext>>) -> Option<String> {
    match tenant_ctx.as_ref() {
        Some(t) if !t.0 .0.is_default => Some(format!("{}:", t.0.tenant_id())),
        _ => None,
    }
}

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
    tenant_ctx: Option<Extension<RequestTenantContext>>,
    Json(request): Json<CreateTemplateRequest>,
) -> Result<(StatusCode, Json<Template>), (StatusCode, Json<TemplateErrorResponse>)> {
    let mut template: Template = request.into();
    // Validate with original ID first (before tenant prefixing, since prefix contains ':')
    if let Err(e) = template.validate() {
        return Err(e.into());
    }
    template.id = tenant_template_id(&tenant_ctx, &template.id);

    match state.template_store.create(template) {
        Ok(created) => Ok((StatusCode::CREATED, Json(created))),
        Err(e) => Err(e.into()),
    }
}

/// GET /api/v1/templates - List all templates
#[tracing::instrument(name = "http.list_templates", skip(state))]
pub async fn list_templates(
    State(state): State<AppState>,
    tenant_ctx: Option<Extension<RequestTenantContext>>,
) -> Json<TemplateListResponse> {
    let all_templates = state.template_store.list();
    let templates: Vec<Template> = match tenant_prefix(&tenant_ctx) {
        Some(prefix) => all_templates
            .into_iter()
            .filter(|t| t.id.starts_with(&prefix))
            .collect(),
        None if state.tenant_manager.is_enabled() => {
            // Default tenant: only show templates without tenant prefix
            all_templates
                .into_iter()
                .filter(|t| !t.id.contains(':'))
                .collect()
        }
        None => all_templates,
    };
    let total = templates.len();

    Json(TemplateListResponse { templates, total })
}

/// GET /api/v1/templates/:id - Get a specific template
#[tracing::instrument(name = "http.get_template", skip(state))]
pub async fn get_template(
    State(state): State<AppState>,
    tenant_ctx: Option<Extension<RequestTenantContext>>,
    Path(id): Path<String>,
) -> Result<Json<Template>, (StatusCode, Json<TemplateErrorResponse>)> {
    let scoped_id = tenant_template_id(&tenant_ctx, &id);
    match state.template_store.get(&scoped_id) {
        Ok(template) => Ok(Json(template)),
        Err(e) => Err(e.into()),
    }
}

/// PUT /api/v1/templates/:id - Update an existing template
#[tracing::instrument(name = "http.update_template", skip(state, request))]
pub async fn update_template(
    State(state): State<AppState>,
    tenant_ctx: Option<Extension<RequestTenantContext>>,
    Path(id): Path<String>,
    Json(request): Json<UpdateTemplateRequest>,
) -> Result<Json<Template>, (StatusCode, Json<TemplateErrorResponse>)> {
    let scoped_id = tenant_template_id(&tenant_ctx, &id);
    match state.template_store.update(&scoped_id, request) {
        Ok(updated) => Ok(Json(updated)),
        Err(e) => Err(e.into()),
    }
}

/// DELETE /api/v1/templates/:id - Delete a template
#[tracing::instrument(name = "http.delete_template", skip(state))]
pub async fn delete_template(
    State(state): State<AppState>,
    tenant_ctx: Option<Extension<RequestTenantContext>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<TemplateErrorResponse>)> {
    let scoped_id = tenant_template_id(&tenant_ctx, &id);
    match state.template_store.delete(&scoped_id) {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(e) => Err(e.into()),
    }
}
