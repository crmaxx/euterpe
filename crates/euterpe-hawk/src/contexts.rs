use serde_json::{Value, json};

use crate::config::HawkConfig;

pub fn build_base_context(config: &HawkConfig) -> Value {
    let hostname = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "unknown".into());
    json!({
        "environment": config.environment,
        "runtime": {
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "hostname": hostname,
        }
    })
}
