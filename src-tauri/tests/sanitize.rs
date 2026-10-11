//! T010: cross-OS file name rules (research R10). Output must be identical on every OS.

use music_organizer_lib::core::sanitize::{
    MAX_COMPONENT_CHARS, MAX_PATH_CHARS, PathError, build_path, sanitize_component,
};
use std::path::Path;

#[test]
fn removes_forbidden_characters() {
    assert_eq!(
        sanitize_component(r#"AC/DC: Back <In> "Black"? | * \ "#),
        "ACDC Back In Black"
    );
}

#[test]
fn removes_control_characters() {
    assert_eq!(
        sanitize_component("Tab\there\u{0}nul\u{1f}x"),
        "Tabherenulx"
    );
}

#[test]
fn renames_windows_reserved_names_case_insensitively() {
    for name in ["CON", "prn", "Aux", "NUL", "COM1", "com9", "LPT1", "lpt9"] {
        assert_eq!(sanitize_component(name), format!("{name}_"), "{name}");
    }
    // Reserved stem with an extension-like suffix is also reserved on Windows.
    assert_eq!(sanitize_component("con.live"), "con_.live");
    // Not reserved: longer names and COM0/COM10.
    assert_eq!(sanitize_component("Console"), "Console");
    assert_eq!(sanitize_component("COM10"), "COM10");
}

#[test]
fn strips_trailing_dots_and_spaces_and_collapses_whitespace() {
    assert_eq!(sanitize_component("  Hello   World . . "), "Hello World");
    assert_eq!(sanitize_component("Mr..."), "Mr");
}

#[test]
fn empty_after_cleaning_becomes_placeholder() {
    assert_eq!(sanitize_component("???"), "Unknown");
    assert_eq!(sanitize_component(""), "Unknown");
}

#[test]
fn limits_component_length_on_char_boundary() {
    let long = "é".repeat(300);
    let out = sanitize_component(&long);
    assert_eq!(out.chars().count(), MAX_COMPONENT_CHARS);
    assert!(out.chars().all(|c| c == 'é'));

    // 4-byte characters: also stays within 240 bytes so ext4/APFS (255-byte names) accept it.
    let emoji = "🎵".repeat(200);
    let out = sanitize_component(&emoji);
    assert!(out.len() <= 240);
    assert!(out.chars().all(|c| c == '🎵'));
}

#[test]
fn build_path_joins_sanitized_components() {
    let root = Path::new("/music");
    let p = build_path(
        root,
        &["AC/DC", "Back in Black"],
        "01 - Hells Bells?",
        "m4a",
    )
    .unwrap();
    assert_eq!(
        p,
        root.join("ACDC")
            .join("Back in Black")
            .join("01 - Hells Bells.m4a")
    );
}

#[test]
fn build_path_shortens_to_fit_total_limit() {
    let root = Path::new("/music");
    let dirs = ["a".repeat(110), "b".repeat(110)];
    let p = build_path(
        root,
        &[dirs[0].as_str(), dirs[1].as_str()],
        &"t".repeat(110),
        "m4a",
    )
    .unwrap();
    assert!(
        p.to_string_lossy().chars().count() <= MAX_PATH_CHARS,
        "{}",
        p.display()
    );
    assert!(p.to_string_lossy().ends_with(".m4a"));
}

#[test]
fn build_path_reports_too_long_when_root_alone_is_too_long() {
    let root_str = format!("/{}", "r".repeat(MAX_PATH_CHARS));
    let err = build_path(Path::new(&root_str), &["Artist"], "Title", "m4a").unwrap_err();
    assert_eq!(err, PathError::TooLong);
}
