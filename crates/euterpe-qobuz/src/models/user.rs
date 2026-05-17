use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct UserProfile {
    pub id: u64,
    pub email: Option<String>,
    #[serde(rename = "display_name")]
    pub display_name: Option<String>,
    pub credential: Option<UserCredential>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserCredential {
    pub parameters: Option<CredentialParameters>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CredentialParameters {
    #[serde(rename = "short_label")]
    pub short_label: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoginResponse {
    #[serde(rename = "user_auth_token")]
    pub user_auth_token: String,
    pub user: LoginUser,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoginUser {
    pub id: u64,
    pub email: Option<String>,
    #[serde(rename = "display_name")]
    pub display_name: Option<String>,
    pub credential: UserCredential,
}

impl LoginResponse {
    pub fn into_profile(self) -> UserProfile {
        UserProfile {
            id: self.user.id,
            email: self.user.email,
            display_name: self.user.display_name,
            credential: Some(self.user.credential),
        }
    }
}
