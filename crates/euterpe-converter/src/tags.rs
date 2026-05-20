use std::path::Path;

use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::prelude::ItemKey;
use lofty::probe::Probe;
use lofty::tag::{Accessor, Tag, TagType};

use crate::error::{ConvertError, Result};

pub fn transfer_tags(src: &Path, dst_flac: &Path) -> Result<()> {
    let tagged = Probe::open(src)
        .map_err(|e| ConvertError::Tags(e.to_string()))?
        .guess_file_type()
        .map_err(|e| ConvertError::Tags(e.to_string()))?
        .read()
        .map_err(|e| ConvertError::Tags(e.to_string()))?;

    let src_tag = tagged.primary_tag().or_else(|| tagged.tags().first());
    let Some(src_tag) = src_tag else {
        return Ok(());
    };

    let mut dst = Probe::open(dst_flac)
        .map_err(|e| ConvertError::Tags(e.to_string()))?
        .guess_file_type()
        .map_err(|e| ConvertError::Tags(e.to_string()))?
        .read()
        .map_err(|e| ConvertError::Tags(e.to_string()))?;

    let mut dst_tag = dst
        .primary_tag()
        .cloned()
        .unwrap_or_else(|| Tag::new(TagType::VorbisComments));

    if let Some(title) = src_tag.title() {
        dst_tag.set_title(title.to_string());
    }
    if let Some(artist) = src_tag.artist() {
        dst_tag.set_artist(artist.to_string());
    }
    if let Some(album) = src_tag.album() {
        dst_tag.set_album(album.to_string());
    }
    if let Some(year) = src_tag.year() {
        dst_tag.set_year(year);
    }
    if let Some(n) = src_tag.track() {
        dst_tag.set_track(n);
    }
    if let Some(n) = src_tag.track_total() {
        dst_tag.set_track_total(n);
    }
    if let Some(n) = src_tag.disk() {
        dst_tag.set_disk(n);
    }
    if let Some(n) = src_tag.disk_total() {
        dst_tag.set_disk_total(n);
    }
    if let Some(genre) = src_tag.genre() {
        dst_tag.set_genre(genre.to_string());
    }
    if let Some(c) = src_tag.comment() {
        dst_tag.insert_text(ItemKey::Comment, c.to_string());
    }
    if let Some(label) = src_tag.get_string(&ItemKey::Label) {
        dst_tag.insert_text(ItemKey::Label, label.to_string());
    }
    if let Some(isrc) = src_tag.get_string(&ItemKey::Isrc) {
        dst_tag.insert_text(ItemKey::Isrc, isrc.to_string());
    }
    if let Some(composer) = src_tag.get_string(&ItemKey::Composer) {
        dst_tag.insert_text(ItemKey::Composer, composer.to_string());
    }

    dst.insert_tag(dst_tag);
    dst.save_to_path(dst_flac, WriteOptions::default())
        .map_err(|e| ConvertError::Tags(e.to_string()))?;
    Ok(())
}
