//! Cross-OS file name rules (research R10). The same rules apply on every OS so a library moved
//! between Windows, Linux and macOS keeps identical paths.

use std::path::{Path, PathBuf};

/// Maximum characters in one path component.
pub const MAX_COMPONENT_CHARS: usize = 120;
/// Maximum bytes in one path component (ext4/APFS allow 255; leave headroom for " (2)").
const MAX_COMPONENT_BYTES: usize = 240;
/// Maximum characters in a full path (Windows MAX_PATH minus headroom), applied on every OS.
pub const MAX_PATH_CHARS: usize = 240;
/// Components are never shortened below this when fitting a path.
const MIN_SHORTENED_CHARS: usize = 16;
const PLACEHOLDER: &str = "Unknown";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PathError {
    #[error("path is too long for this file system")]
    TooLong,
}

const FORBIDDEN: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

fn is_reserved_windows_name(stem: &str) -> bool {
    if !stem.is_ascii() || !(3..=4).contains(&stem.len()) {
        return false;
    }
    let upper = stem.to_ascii_uppercase();
    match upper.as_bytes() {
        b"CON" | b"PRN" | b"AUX" | b"NUL" => true,
        [b'C', b'O', b'M', d] | [b'L', b'P', b'T', d] => (b'1'..=b'9').contains(d),
        _ => false,
    }
}

/// Make one path component safe on Windows, Linux and macOS.
pub fn sanitize_component(input: &str) -> String {
    let cleaned: String = input
        .chars()
        .filter(|c| !c.is_control() && !FORBIDDEN.contains(c))
        .collect();
    let mut out = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");

    truncate(&mut out, MAX_COMPONENT_CHARS);
    trim_trailing_dots_and_spaces(&mut out);

    let stem_end = out.find('.').unwrap_or(out.len());
    if is_reserved_windows_name(&out[..stem_end]) {
        out.insert(stem_end, '_');
    }
    if out.is_empty() {
        return PLACEHOLDER.to_string();
    }
    out
}

/// Truncate to `max_chars` characters and [`MAX_COMPONENT_BYTES`] bytes on a char boundary.
fn truncate(s: &mut String, max_chars: usize) {
    let mut end = 0;
    for (count, (idx, ch)) in s.char_indices().enumerate() {
        if count == max_chars || idx + ch.len_utf8() > MAX_COMPONENT_BYTES {
            break;
        }
        end = idx + ch.len_utf8();
    }
    s.truncate(end);
}

fn trim_trailing_dots_and_spaces(s: &mut String) {
    let trimmed = s.trim_end_matches(['.', ' ']).len();
    s.truncate(trimmed);
}

/// Build `root/dir1/dir2/stem.ext` from unsanitized parts, shortening components so the full
/// path fits in [`MAX_PATH_CHARS`]. The file stem is shortened first, then the directories.
pub fn build_path(root: &Path, dirs: &[&str], stem: &str, ext: &str) -> Result<PathBuf, PathError> {
    let mut parts: Vec<String> = dirs.iter().map(|d| sanitize_component(d)).collect();
    parts.push(sanitize_component(stem));
    let suffix_len = if ext.is_empty() {
        0
    } else {
        ext.chars().count() + 1
    };

    let len = |parts: &[String]| {
        let root_len = root.to_string_lossy().chars().count();
        root_len + parts.iter().map(|p| p.chars().count() + 1).sum::<usize>() + suffix_len
    };

    // Shorten the stem, then directories from the deepest up, until the path fits.
    let order: Vec<usize> = (0..parts.len()).rev().collect();
    for &i in &order {
        let excess = len(&parts).saturating_sub(MAX_PATH_CHARS);
        if excess == 0 {
            break;
        }
        let cur = parts[i].chars().count();
        let target = cur.saturating_sub(excess).max(MIN_SHORTENED_CHARS.min(cur));
        let mut shortened = parts[i].clone();
        truncate(&mut shortened, target);
        trim_trailing_dots_and_spaces(&mut shortened);
        if shortened.is_empty() {
            shortened = PLACEHOLDER.to_string();
        }
        parts[i] = shortened;
    }
    if len(&parts) > MAX_PATH_CHARS {
        return Err(PathError::TooLong);
    }

    let file_name = parts.pop().map(|s| {
        if ext.is_empty() {
            s
        } else {
            format!("{s}.{ext}")
        }
    });
    let mut path = root.to_path_buf();
    path.extend(parts);
    path.extend(file_name);
    Ok(path)
}
