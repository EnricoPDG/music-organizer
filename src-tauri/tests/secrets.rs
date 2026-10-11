//! T014: secrets never leak through Debug, Display or serialization.

use music_organizer_lib::secrets::Secret;

#[test]
fn debug_and_display_are_redacted() {
    let s = Secret::new("super-token-123".to_string());
    assert_eq!(format!("{s:?}"), "[REDACTED]");
    assert_eq!(format!("{s}"), "[REDACTED]");
    assert_eq!(s.expose(), "super-token-123");
}

#[test]
fn serialization_is_redacted() {
    #[derive(serde::Serialize)]
    struct Tokens {
        access_token: Secret<String>,
    }
    let json = serde_json::to_string(&Tokens {
        access_token: Secret::new("abc".into()),
    })
    .unwrap();
    assert_eq!(json, r#"{"access_token":"[REDACTED]"}"#);
    assert!(!json.contains("abc"));
}

#[test]
fn tracing_output_is_redacted() {
    let s = Secret::new("leaky".to_string());
    let line = format!("connected token={s:?} again={s}");
    assert!(!line.contains("leaky"));
}
