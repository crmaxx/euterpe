use std::collections::VecDeque;
use std::cell::RefCell;

use serde_json::{json, Value};
use uuid::Uuid;

use crate::event::AffectedUser;
use crate::http_addons::build_http_addons;

const MAX_BREADCRUMBS: usize = 20;

#[derive(Clone, Debug)]
pub struct Breadcrumb {
    pub level: String,
    pub message: String,
    pub target: String,
}

#[derive(Clone, Debug)]
pub struct HawkScope {
    pub request_id: String,
    pub method: Option<String>,
    pub uri: Option<String>,
    pub headers: Option<serde_json::Map<String, Value>>,
    pub user: Option<AffectedUser>,
    pub extra: serde_json::Map<String, Value>,
}

impl HawkScope {
    pub fn new_http(
        method: String,
        uri: String,
        headers: serde_json::Map<String, Value>,
    ) -> Self {
        Self {
            request_id: Uuid::new_v4().to_string(),
            method: Some(method),
            uri: Some(uri),
            headers: Some(headers),
            user: None,
            extra: serde_json::Map::new(),
        }
    }

    pub fn context_value(&self, breadcrumbs: &[Breadcrumb]) -> Value {
        let mut map = serde_json::Map::new();
        map.insert("request_id".into(), json!(self.request_id));
        if !self.extra.is_empty() {
            map.insert("extra".into(), Value::Object(self.extra.clone()));
        }
        if !breadcrumbs.is_empty() {
            let crumbs: Vec<Value> = breadcrumbs
                .iter()
                .map(|b| {
                    json!({
                        "level": b.level,
                        "message": b.message,
                        "target": b.target,
                    })
                })
                .collect();
            map.insert("breadcrumbs".into(), Value::Array(crumbs));
        }
        Value::Object(map)
    }

    pub fn http_addons(&self) -> Option<Value> {
        let method = self.method.as_deref()?;
        let uri = self.uri.as_deref()?;
        let headers = self.headers.clone().unwrap_or_default();
        Some(build_http_addons(method, uri, headers))
    }
}

tokio::task_local! {
    static CURRENT_SCOPE: HawkScope;
}

thread_local! {
    static BREADCRUMBS: RefCell<VecDeque<Breadcrumb>> = const { RefCell::new(VecDeque::new()) };
}

pub fn clear_breadcrumbs() {
    BREADCRUMBS.with(|cell| cell.borrow_mut().clear());
}

pub fn add_breadcrumb(level: &str, message: String, target: String) {
    BREADCRUMBS.with(|cell| {
        let mut crumbs = cell.borrow_mut();
        if crumbs.len() >= MAX_BREADCRUMBS {
            crumbs.pop_front();
        }
        crumbs.push_back(Breadcrumb {
            level: level.to_string(),
            message,
            target,
        });
    });
}

pub fn take_breadcrumbs() -> Vec<Breadcrumb> {
    BREADCRUMBS.with(|cell| cell.borrow_mut().drain(..).collect())
}

pub async fn with_scope<F, R>(scope: HawkScope, f: F) -> R
where
    F: std::future::Future<Output = R>,
{
    clear_breadcrumbs();
    CURRENT_SCOPE.scope(scope, f).await
}

pub fn try_with_scope<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&HawkScope) -> R,
{
    CURRENT_SCOPE.try_with(f).ok()
}

pub fn current_http_addons() -> Option<Value> {
    try_with_scope(|s| s.http_addons()).flatten()
}

pub fn current_scope_context() -> Option<Value> {
    try_with_scope(|s| {
        let crumbs = take_breadcrumbs();
        Some(s.context_value(&crumbs))
    })
    .flatten()
}

pub fn current_scope_user() -> Option<AffectedUser> {
    try_with_scope(|s| s.user.clone()).flatten()
}
