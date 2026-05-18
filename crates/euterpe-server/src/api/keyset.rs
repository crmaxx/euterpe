//! Keyset (seek) pagination helpers shared by list endpoints (FP-8).

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::ApiError;

const CURSOR_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    #[default]
    Asc,
    Desc,
}

impl SortOrder {
    pub fn parse(s: &str) -> Result<Self, ApiError> {
        match s {
            "asc" => Ok(Self::Asc),
            "desc" => Ok(Self::Desc),
            _ => Err(ApiError::bad_request("order must be asc or desc")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeysetPage<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorPayload {
    v: u32,
    sort: String,
    order: SortOrder,
    fingerprint: String,
    keys: Value,
}

#[derive(Debug, Clone)]
pub enum SortKeyValue {
    Text(String),
    Int(i64),
    Bool(i32),
}

#[derive(Debug, Clone, Copy)]
pub enum SortKeyKind {
    Text,
    Int,
    Bool,
}

impl SortKeyValue {
    pub fn from_json(value: &Value, kind: SortKeyKind) -> Result<Self, ApiError> {
        match kind {
            SortKeyKind::Text => {
                let s = value
                    .as_str()
                    .ok_or_else(|| ApiError::invalid_cursor("invalid text key in cursor"))?;
                Ok(Self::Text(s.to_string()))
            }
            SortKeyKind::Int => {
                let n = value
                    .as_i64()
                    .ok_or_else(|| ApiError::invalid_cursor("invalid int key in cursor"))?;
                Ok(Self::Int(n))
            }
            SortKeyKind::Bool => {
                let b = value
                    .as_i64()
                    .ok_or_else(|| ApiError::invalid_cursor("invalid bool key in cursor"))?;
                if b != 0 && b != 1 {
                    return Err(ApiError::invalid_cursor("invalid bool key in cursor"));
                }
                Ok(Self::Bool(b as i32))
            }
        }
    }

    pub fn primary_json(&self) -> Value {
        match self {
            Self::Text(s) => Value::String(s.clone()),
            Self::Int(n) => Value::from(*n),
            Self::Bool(b) => Value::from(*b),
        }
    }
}

/// Stable fingerprint for filter query params (client must reset cursor when this changes).
pub fn fingerprint_json(filters: &Value) -> String {
    serde_json::to_string(filters).unwrap_or_else(|_| "{}".to_string())
}

pub fn parse_limit(limit: u32, default: u32, max: u32) -> Result<u32, ApiError> {
    let limit = if limit == 0 { default } else { limit };
    if limit == 0 || limit > max {
        return Err(ApiError::bad_request(format!("limit must be 1..={max}")));
    }
    Ok(limit)
}

pub fn decode_cursor(cursor: &str) -> Result<CursorPayload, ApiError> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(cursor)
        .map_err(|_| ApiError::invalid_cursor("invalid cursor encoding"))?;
    let payload: CursorPayload = serde_json::from_slice(&bytes)
        .map_err(|_| ApiError::invalid_cursor("invalid cursor payload"))?;
    if payload.v != CURSOR_VERSION {
        return Err(ApiError::invalid_cursor("unsupported cursor version"));
    }
    Ok(payload)
}

pub fn encode_cursor(
    sort: &str,
    order: SortOrder,
    fingerprint: &str,
    primary: &SortKeyValue,
    tie_breaker: i64,
) -> String {
    let keys = serde_json::json!({
        "primary": primary.primary_json(),
        "tie": tie_breaker,
    });
    let payload = CursorPayload {
        v: CURSOR_VERSION,
        sort: sort.to_string(),
        order,
        fingerprint: fingerprint.to_string(),
        keys,
    };
    let bytes = serde_json::to_vec(&payload).expect("cursor json");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub fn ensure_cursor_matches(
    cursor: &CursorPayload,
    sort: &str,
    order: SortOrder,
    fingerprint: &str,
    primary_kind: SortKeyKind,
) -> Result<(SortKeyValue, i64), ApiError> {
    if cursor.sort != sort {
        return Err(ApiError::invalid_cursor("cursor sort does not match query"));
    }
    if cursor.order != order {
        return Err(ApiError::invalid_cursor("cursor order does not match query"));
    }
    if cursor.fingerprint != fingerprint {
        return Err(ApiError::invalid_cursor(
            "cursor does not match current filters; reset pagination",
        ));
    }
    let primary = cursor
        .keys
        .get("primary")
        .ok_or_else(|| ApiError::invalid_cursor("cursor missing primary key"))?;
    let tie = cursor
        .keys
        .get("tie")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ApiError::invalid_cursor("cursor missing tie key"))?;
    let primary = SortKeyValue::from_json(primary, primary_kind)?;
    Ok((primary, tie))
}

/// Build SQL fragment `AND (...)` for keyset continuation, plus bind values in order.
pub fn keyset_and_clause(
    order: SortOrder,
    sort_sql: &str,
    tie_sql: &str,
    primary: &SortKeyValue,
    tie_id: i64,
) -> (String, Vec<SortKeyValue>) {
    let (primary_op, tie_op) = match order {
        SortOrder::Asc => (">", ">"),
        SortOrder::Desc => ("<", ">"),
    };

    let clause = format!(
        "AND ({sort_sql} {primary_op} ? OR ({sort_sql} = ? AND {tie_sql} {tie_op} ?))",
        sort_sql = sort_sql,
        tie_sql = tie_sql,
        primary_op = primary_op,
        tie_op = tie_op,
    );

    let mut binds = vec![primary.clone(), primary.clone()];
    binds.push(SortKeyValue::Int(tie_id));
    (clause, binds)
}

pub fn finish_keyset_page<T, F>(
    mut rows: Vec<T>,
    limit: usize,
    sort: &str,
    order: SortOrder,
    fingerprint: &str,
    extract: F,
) -> KeysetPage<T>
where
    F: Fn(&T) -> (SortKeyValue, i64),
{
    let has_more = rows.len() > limit;
    if has_more {
        rows.truncate(limit);
    }
    let next_cursor = if has_more {
        rows.last().map(|row| {
            let (primary, tie) = extract(row);
            encode_cursor(sort, order, fingerprint, &primary, tie)
        })
    } else {
        None
    };
    KeysetPage {
        items: rows,
        next_cursor,
        has_more,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_round_trip() {
        let primary = SortKeyValue::Text("Beta".into());
        let c = encode_cursor("title", SortOrder::Asc, "fp1", &primary, 42);
        let decoded = decode_cursor(&c).unwrap();
        assert_eq!(decoded.sort, "title");
        let (p, tie) =
            ensure_cursor_matches(&decoded, "title", SortOrder::Asc, "fp1", SortKeyKind::Text)
                .unwrap();
        assert_eq!(p.primary_json(), primary.primary_json());
        assert_eq!(tie, 42);
    }

    #[test]
    fn fingerprint_mismatch_errors() {
        let primary = SortKeyValue::Text("A".into());
        let c = encode_cursor("title", SortOrder::Asc, "fp1", &primary, 1);
        let decoded = decode_cursor(&c).unwrap();
        assert!(
            ensure_cursor_matches(&decoded, "title", SortOrder::Asc, "fp2", SortKeyKind::Text)
                .is_err()
        );
    }
}
