//! Build [`TrackTags`] and DB metadata from Qobuz `album/get` models.

use euterpe_qobuz::{AlbumDetail, GenreRef, TrackSummary};

use crate::library::paths::year_from_release_date;
use crate::library::tags::TrackTags;

pub fn track_tags_from_qobuz(
    album: &AlbumDetail,
    track: &TrackSummary,
    catalog_album_id: u64,
) -> TrackTags {
    let year = year_from_release_date(album.summary.release_date_original.as_deref())
        .map(|y| y as u32);

    TrackTags {
        title: track.title.clone(),
        artist: resolve_artist(album, track),
        album: album.summary.title.clone(),
        track_number: track.track_number,
        year,
        disc_number: track.media_number,
        track_total: None,
        disc_total: None,
        genre: resolve_genre(track.genre.as_ref(), album.summary.genre.as_ref()),
        duration_sec: None,
        qobuz_track_id: Some(track.id),
        qobuz_album_id: Some(catalog_album_id),
        label: album.summary.label.as_ref().and_then(|l| l.display_name().map(str::to_string)),
        isrc: track
            .isrc
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        composer: track
            .composer
            .as_ref()
            .map(|c| c.name.trim())
            .filter(|s| !s.is_empty())
            .map(str::to_string),
    }
}

pub fn track_db_fields_from_qobuz(
    album: &AlbumDetail,
    track: &TrackSummary,
) -> (Option<i32>, Option<String>) {
    let disc = track.media_number.map(|n| n as i32);
    let genre = resolve_genre(track.genre.as_ref(), album.summary.genre.as_ref());
    (disc, genre)
}

fn resolve_artist(album: &AlbumDetail, track: &TrackSummary) -> String {
    if let Some(a) = album.summary.artist.as_ref() {
        let name = a.name.trim();
        if !name.is_empty() {
            return name.to_string();
        }
    }
    if let Some(p) = track.performer.as_ref() {
        let name = p.name.trim();
        if !name.is_empty() {
            return name.to_string();
        }
    }
    "Unknown Artist".to_string()
}

fn resolve_genre(
    track_genre: Option<&GenreRef>,
    album_genre: Option<&GenreRef>,
) -> Option<String> {
    track_genre
        .and_then(|g| g.display_name())
        .or_else(|| album_genre.and_then(|g| g.display_name()))
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use euterpe_qobuz::{AlbumDetail, AlbumSummary, AlbumTracks, ArtistRef, GenreRef, LabelRef, TrackSummary};

    use super::*;

    fn rich_album() -> AlbumDetail {
        AlbumDetail {
            summary: AlbumSummary {
                id: 99,
                qobuz_id: None,
                title: "Rich Album".into(),
                artist: Some(ArtistRef {
                    id: 1,
                    name: "Album Artist".into(),
                }),
                artists: None,
                image: None,
                release_date_original: Some("2019-06-01".into()),
                hires: None,
                album_ref: None,
                slug: None,
                list_id: None,
                product_id: None,
                genre: Some(GenreRef {
                    id: Some(1),
                    name: "Classical".into(),
                }),
                label: Some(LabelRef {
                    id: Some(2),
                    name: "Test Label".into(),
                }),
            },
            tracks: Some(AlbumTracks {
                items: vec![
                    TrackSummary {
                        id: 1001,
                        title: "One".into(),
                        track_number: Some(1),
                        duration: Some(200),
                        performer: Some(ArtistRef {
                            id: 3,
                            name: "Soloist".into(),
                        }),
                        hires_streamable: None,
                        media_number: Some(2),
                        genre: Some(GenreRef {
                            id: None,
                            name: "Orchestral".into(),
                        }),
                        isrc: Some("XX-1".into()),
                        composer: Some(ArtistRef {
                            id: 4,
                            name: "Composer".into(),
                        }),
                    },
                    TrackSummary {
                        id: 1002,
                        title: "Two".into(),
                        track_number: Some(2),
                        duration: None,
                        performer: None,
                        hires_streamable: None,
                        media_number: None,
                        genre: None,
                        isrc: None,
                        composer: None,
                    },
                ],
            }),
            description: None,
        }
    }

    #[test]
    fn artist_prefers_album_over_performer() {
        let album = rich_album();
        let track = &album.tracks.as_ref().unwrap().items[0];
        let tags = track_tags_from_qobuz(&album, track, 42);
        assert_eq!(tags.artist, "Album Artist");
    }

    #[test]
    fn artist_falls_back_to_performer() {
        let mut album = rich_album();
        album.summary.artist = None;
        let track = &album.tracks.as_ref().unwrap().items[0];
        let tags = track_tags_from_qobuz(&album, track, 42);
        assert_eq!(tags.artist, "Soloist");
    }

    #[test]
    fn genre_prefers_track_over_album() {
        let album = rich_album();
        let track = &album.tracks.as_ref().unwrap().items[0];
        let tags = track_tags_from_qobuz(&album, track, 42);
        assert_eq!(tags.genre.as_deref(), Some("Orchestral"));
    }

    #[test]
    fn genre_falls_back_to_album() {
        let album = rich_album();
        let track = &album.tracks.as_ref().unwrap().items[1];
        let tags = track_tags_from_qobuz(&album, track, 42);
        assert_eq!(tags.genre.as_deref(), Some("Classical"));
    }

    #[test]
    fn year_and_qobuz_ids() {
        let album = rich_album();
        let track = &album.tracks.as_ref().unwrap().items[0];
        let tags = track_tags_from_qobuz(&album, track, 4242);
        assert_eq!(tags.year, Some(2019));
        assert_eq!(tags.qobuz_track_id, Some(1001));
        assert_eq!(tags.qobuz_album_id, Some(4242));
        assert_eq!(tags.disc_number, Some(2));
        assert_eq!(tags.label.as_deref(), Some("Test Label"));
        assert_eq!(tags.isrc.as_deref(), Some("XX-1"));
        assert_eq!(tags.composer.as_deref(), Some("Composer"));
    }

    #[test]
    fn db_fields_match_tags() {
        let album = rich_album();
        let track = &album.tracks.as_ref().unwrap().items[0];
        let (disc, genre) = track_db_fields_from_qobuz(&album, track);
        assert_eq!(disc, Some(2));
        assert_eq!(genre.as_deref(), Some("Orchestral"));
    }
}
