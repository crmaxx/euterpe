use serde_json::{Value, json};

pub fn build_http_addons(
    method: &str,
    uri: &str,
    headers: serde_json::Map<String, Value>,
) -> Value {
    json!({
        "rust": {
            "method": method,
            "url": uri,
            "headers": headers,
        }
    })
}

pub fn panic_mechanism_addon() -> Value {
    json!({
        "mechanism": {
            "type": "panic",
            "handled": false
        }
    })
}
