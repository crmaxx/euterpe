use euterpe_qobuz::{sign_favorites, sign_track_file_url};

#[test]
fn track_get_file_url_signature() {
    let sig = sign_track_file_url(6, 12_345_678, 1_715_900_000, "test_secret");
    assert_eq!(sig, "968e435667d547d87708bc87e09ce2d0");
}

#[test]
fn favorites_timestamp_only_signature() {
    let sig = sign_favorites(1_715_900_000, None);
    assert_eq!(sig, "1d50d7893bc7009d1930672d55127ba7");
}

#[test]
fn favorites_timestamp_secret_signature() {
    let sig = sign_favorites(1_715_900_000, Some("test_secret"));
    assert_eq!(sig, "bdfd6f5af83e82b8052b74cfbafeb992");
}
