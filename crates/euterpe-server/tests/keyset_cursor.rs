use euterpe_server::api::keyset::{decode_cursor, encode_cursor, ensure_cursor_matches, SortKeyKind, SortKeyValue, SortOrder};
use serde_json::json;

#[test]
fn cursor_round_trip_and_schema_fields() {
    let primary = SortKeyValue::Bool(1);
    let encoded = encode_cursor("in_library", SortOrder::Desc, "fp", &primary, 99);
    let payload = decode_cursor(&encoded).unwrap();
    let (p, tie) = ensure_cursor_matches(
        &payload,
        "in_library",
        SortOrder::Desc,
        "fp",
        SortKeyKind::Bool,
    )
    .unwrap();
    assert_eq!(p.primary_json(), json!(1));
    assert_eq!(tie, 99);
}
