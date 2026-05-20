use crate::event::ErrorReport;

const SENSITIVE_KEY_PARTS: &[&str] = &[
    "password",
    "secret",
    "passwd",
    "api_key",
    "apikey",
    "access_token",
    "auth",
    "credentials",
    "authorization",
    "cookie",
    "stripetoken",
    "card",
    "cardnumber",
];

/// Default `before_send`: strip known sensitive header keys and context fields.
pub fn default_before_send(event: &mut ErrorReport) {
    if let Some(addons) = event.payload.addons.as_mut() {
        redact_value(addons);
    }
    if let Some(ctx) = event.payload.context.as_mut() {
        redact_value(ctx);
    }
}

pub fn redact_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            let keys: Vec<String> = map.keys().cloned().collect();
            for key in keys {
                if is_sensitive_key(&key) {
                    map.remove(&key);
                } else if let Some(child) = map.get_mut(&key) {
                    redact_value(child);
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                redact_value(item);
            }
        }
        _ => {}
    }
}

pub fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    SENSITIVE_KEY_PARTS
        .iter()
        .any(|part| lower.contains(part))
}

#[cfg(feature = "axum")]
pub fn sanitize_header_map(
    headers: impl Iterator<Item = (String, String)>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::new();
    for (name, value) in headers {
        if is_sensitive_key(&name) {
            map.insert(name, serde_json::Value::String("[filtered]".into()));
        } else {
            map.insert(name, serde_json::Value::String(value));
        }
    }
    map
}
