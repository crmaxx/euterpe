use serde::Serialize;
use serde_json::Value;

use crate::CATCHER_TYPE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventLevel {
    Warning,
    Error,
    Fatal,
}

impl EventLevel {
    pub fn as_u16(self) -> u16 {
        match self {
            EventLevel::Warning => 4,
            EventLevel::Error => 8,
            EventLevel::Fatal => 16,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorReport {
    pub token: String,
    pub catcher_type: String,
    pub payload: Payload,
}

impl ErrorReport {
    pub fn new(token: impl Into<String>, payload: Payload) -> Self {
        Self {
            token: token.into(),
            catcher_type: CATCHER_TYPE.to_string(),
            payload,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AffectedUser {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(rename = "photo", skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

impl AffectedUser {
    pub fn is_empty(&self) -> bool {
        self.id.as_ref().is_none_or(|s| s.is_empty())
            && self.name.as_ref().is_none_or(|s| s.is_empty())
            && self.url.as_ref().is_none_or(|s| s.is_empty())
            && self.image.as_ref().is_none_or(|s| s.is_empty())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Payload {
    pub title: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub backtrace: Vec<BacktraceFrame>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<AffectedUser>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<Value>,
    pub catcher_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addons: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BacktraceFrame {
    pub file: String,
    pub line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub source_code: Vec<SourceLine>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceLine {
    pub line: u32,
    pub content: String,
}
