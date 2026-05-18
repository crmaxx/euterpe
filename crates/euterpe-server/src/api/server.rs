use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ServerInfoResponse {
    pub version: String,
    pub library_path: String,
    pub credentials_configured: bool,
    pub admin_auth_required: bool,
}
