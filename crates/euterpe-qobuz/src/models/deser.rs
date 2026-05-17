use serde::de::{self, Deserializer};
use serde::Deserialize;

/// Qobuz API returns numeric ids as JSON numbers or strings (often zero-padded).
use serde_json::Value;

/// Short opaque album id for `album/get` (e.g. `zg7pv28g4mldg`), when JSON `id` is not numeric.
pub fn parse_album_ref_value(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => {
            let t = s.trim();
            if t.is_empty() || t.chars().all(|c| c.is_ascii_digit()) {
                None
            } else {
                Some(t.to_string())
            }
        }
        _ => None,
    }
}

pub fn parse_id_value(v: &Value) -> Result<u64, String> {
    match v {
        Value::Null => Err("null id".into()),
        Value::Number(n) => n
            .as_u64()
            .ok_or_else(|| format!("unsupported number {n}")),
        Value::String(s) => {
            let t = s.trim();
            if t.is_empty() {
                return Err("empty id".into());
            }
            t.parse::<u64>().map_err(|e| format!("{e} in '{t}'"))
        }
        _ => Err(format!("unexpected id json type: {v}")),
    }
}

pub fn deserialize_qobuz_id<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    parse_id_value(&Value::deserialize(deserializer)?).map_err(de::Error::custom)
}

/// Qobuz JSON often uses `null` where Rust models use empty strings.
pub fn deserialize_null_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

pub fn deserialize_opt_f64<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Num {
        F(f64),
        U(u64),
        I(i64),
    }

    let opt = Option::<Num>::deserialize(deserializer)?;
    Ok(opt.map(|n| match n {
        Num::F(f) => f,
        Num::U(u) => u as f64,
        Num::I(i) => i as f64,
    }))
}
