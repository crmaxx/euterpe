//! Real-network magnet inspect. Run locally with:
//! `cargo test -p euterpe-torrent --test magnet_integration -- --ignored`

use euterpe_torrent::SessionSettings;
use euterpe_torrent::{LibrqbitEngine, TorrentEngine, TorrentEngineConfig};

#[tokio::test]
#[ignore = "requires DHT/network; set EUTERPE_TEST_MAGNET to run"]
async fn test_inspect_magnet_list_only() {
    let magnet = std::env::var("EUTERPE_TEST_MAGNET")
        .expect("set EUTERPE_TEST_MAGNET to a well-known torrent magnet");
    let dir = std::env::temp_dir().join("euterpe-torrent-magnet-test");
    let engine = LibrqbitEngine::new(TorrentEngineConfig {
        incoming_dir: dir.clone(),
        session_settings: SessionSettings {
            disable_upload: true,
            upload_bps: None,
            download_bps: None,
            enable_upnp_port_forwarding: false,
        },
    })
    .await
    .expect("engine");
    let staging = dir.join("staging");
    let result = engine
        .inspect_magnet(&magnet, staging)
        .await
        .expect("inspect");
    assert!(!result.files.is_empty());
    assert!(!result.info_hash_v1.is_empty());
}
