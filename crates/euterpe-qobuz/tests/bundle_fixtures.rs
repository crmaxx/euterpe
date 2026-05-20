use euterpe_qobuz::bundle::{decode_secrets, parse_app_id_from_bundle};

#[test]
fn parse_app_id_from_bundle_fixture() {
    let bundle = include_str!("fixtures/bundle_sample.js");
    let app_id = parse_app_id_from_bundle(bundle).unwrap();
    assert_eq!(app_id, "123456789");
}

#[test]
fn decode_secrets_golden() {
    let bundle = include_str!("fixtures/bundle_sample.js");
    let secrets = decode_secrets(bundle).unwrap();
    assert_eq!(secrets.len(), 2);
    let mut sorted = secrets;
    sorted.sort();
    assert_eq!(
        sorted,
        vec![
            "test_secret_alpha".to_string(),
            "test_secret_beta".to_string()
        ]
    );
}
