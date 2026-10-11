//! Reading and writing audio tags with lofty (research R5): MP4 atoms, Vorbis comments and
//! ID3v2.4. The source link is stored in the comment field, which every format supports.

use lofty::config::WriteOptions;
use lofty::picture::{Picture, PictureType};
use lofty::prelude::*;
use lofty::tag::{Tag, items::Timestamp};
use std::path::Path;

/// Prefix of the comment that records where a downloaded file came from.
const SOURCE_MARKER: &str = "music-organizer";

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TagSet {
    pub title: String,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub track_number: Option<u32>,
    pub year: Option<u32>,
    /// Image bytes (JPEG/PNG/WebP).
    pub cover: Option<Vec<u8>>,
    pub source_url: Option<String>,
    /// `platform:id`, e.g. `spotify:0VjIjW4GlUZAMYd2vXMi3b`.
    pub source_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ReadTags {
    pub title: Option<String>,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub track_number: Option<u32>,
    pub year: Option<u32>,
    pub cover: Option<Vec<u8>>,
    pub cover_count: usize,
    pub source_url: Option<String>,
    pub source_id: Option<String>,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
pub enum TagError {
    #[error("unsupported or unreadable audio file: {0}")]
    Read(String),
    #[error("could not write tags: {0}")]
    Write(String),
}

impl From<TagError> for crate::error::AppError {
    fn from(e: TagError) -> Self {
        crate::error::AppError::internal(e)
    }
}

/// Replace the app-managed tags in `path`. Other tags the file already has are kept.
pub fn write_tags(path: &Path, t: &TagSet) -> Result<(), TagError> {
    let mut file = lofty::read_from_path(path).map_err(|e| TagError::Read(e.to_string()))?;
    let tag_type = file.primary_tag_type();
    if file.primary_tag_mut().is_none() {
        file.insert_tag(Tag::new(tag_type));
    }
    let tag = file.primary_tag_mut().expect("tag inserted above");

    tag.set_title(t.title.clone());
    // Display artist joined for players that read one value; the full list goes into the
    // multi-value ARTISTS field so names containing commas survive a round trip.
    tag.set_artist(t.artists.join(", "));
    tag.remove_key(ItemKey::TrackArtists);
    for a in &t.artists {
        tag.push(lofty::tag::TagItem::new(
            ItemKey::TrackArtists,
            lofty::tag::ItemValue::Text(a.clone()),
        ));
    }
    set_opt(tag, ItemKey::AlbumTitle, t.album.as_deref());
    set_opt(tag, ItemKey::AlbumArtist, t.album_artist.as_deref());
    match t.track_number {
        Some(n) => tag.set_track(n),
        None => tag.remove_track(),
    }
    match t.year {
        Some(y) => tag.set_date(Timestamp {
            year: y as u16,
            ..Default::default()
        }),
        None => tag.remove_date(),
    }
    let comment = match (&t.source_url, &t.source_id) {
        (None, None) => None,
        (url, id) => Some(format!(
            "{SOURCE_MARKER} source={} id={}",
            url.as_deref().unwrap_or("-"),
            id.as_deref().unwrap_or("-")
        )),
    };
    set_opt(tag, ItemKey::Comment, comment.as_deref());

    if let Some(bytes) = &t.cover {
        let mut pic = Picture::from_reader(&mut bytes.as_slice())
            .map_err(|e| TagError::Write(e.to_string()))?;
        pic.set_pic_type(PictureType::CoverFront);
        if tag_type == lofty::tag::TagType::Mp4Ilst {
            // MP4 "covr" atoms carry no picture type, so every image counts as the cover.
            while !tag.pictures().is_empty() {
                tag.remove_picture(0);
            }
        } else {
            tag.remove_picture_type(PictureType::CoverFront);
        }
        tag.push_picture(pic);
    }

    tag.save_to_path(path, WriteOptions::default())
        .map_err(|e| TagError::Write(e.to_string()))
}

fn set_opt(tag: &mut Tag, key: ItemKey, value: Option<&str>) {
    match value {
        Some(v) if !v.is_empty() => {
            tag.insert_text(key, v.to_string());
        }
        _ => tag.remove_key(key),
    }
}

/// Read tags without modifying the file (scanned files are opened read-only, FR-015).
pub fn read_tags(path: &Path) -> Result<ReadTags, TagError> {
    let file = lofty::read_from_path(path).map_err(|e| TagError::Read(e.to_string()))?;
    let duration = file.properties().duration().as_millis() as u64;
    let mut out = ReadTags {
        duration_ms: (duration > 0).then_some(duration),
        ..Default::default()
    };
    let Some(tag) = file.primary_tag().or_else(|| file.first_tag()) else {
        return Ok(out);
    };

    out.title = tag.title().map(|s| s.to_string()).filter(|s| !s.is_empty());
    let multi: Vec<String> = tag
        .get_strings(ItemKey::TrackArtists)
        .map(str::to_string)
        .collect();
    out.artists = if !multi.is_empty() {
        multi
    } else {
        tag.artist()
            .map(|a| vec![a.to_string()])
            .unwrap_or_default()
    };
    out.album = tag.album().map(|s| s.to_string()).filter(|s| !s.is_empty());
    out.album_artist = tag.get_string(ItemKey::AlbumArtist).map(str::to_string);
    out.track_number = tag.track().filter(|n| *n > 0);
    out.year = tag
        .date()
        .map(|d| u32::from(d.year))
        .filter(|y| (1000..=2100).contains(y));

    let mut covers: Vec<&Picture> = tag
        .pictures()
        .iter()
        .filter(|p| p.pic_type() == PictureType::CoverFront)
        .collect();
    if covers.is_empty() && tag.tag_type() == lofty::tag::TagType::Mp4Ilst {
        covers = tag.pictures().iter().collect();
    }
    out.cover_count = covers.len();
    out.cover = covers
        .first()
        .copied()
        .or(tag.pictures().first())
        .map(|p| p.data().to_vec());

    if let Some(c) = tag.comment().filter(|c| c.starts_with(SOURCE_MARKER)) {
        for part in c.split_whitespace() {
            if let Some(v) = part.strip_prefix("source=").filter(|v| *v != "-") {
                out.source_url = Some(v.to_string());
            }
            if let Some(v) = part.strip_prefix("id=").filter(|v| *v != "-") {
                out.source_id = Some(v.to_string());
            }
        }
    }
    Ok(out)
}
