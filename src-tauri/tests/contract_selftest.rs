//! Proves the contract suite itself works, using a tiny in-memory adapter.

use crate::common::contract::CollectionCase;
use async_trait::async_trait;
use music_organizer_lib::core::model::{
    FetchMode, NormalizedCollection, NormalizedTrack, ResourceKind, ResourceRef,
};
use music_organizer_lib::platforms::{AdapterCtx, AdapterError, Capabilities, PlatformAdapter};
use music_organizer_lib::tools::fake::FakeToolRunner;
use music_organizer_lib::tools::{Tool, ToolRunner};
use std::sync::Arc;
use url::Url;

struct ToyAdapter;

#[async_trait]
impl PlatformAdapter for ToyAdapter {
    fn id(&self) -> &'static str {
        "other"
    }
    fn display_name(&self) -> &'static str {
        "Toy"
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            public_links: true,
            sign_in: false,
            provides_audio: false,
        }
    }
    fn parse_url(&self, url: &Url) -> Option<ResourceRef> {
        (url.host_str() == Some("example.com")).then(|| ResourceRef {
            platform: "other",
            kind: ResourceKind::Playlist,
            id: "1".into(),
            url: url.clone(),
        })
    }
    async fn fetch_collection(
        &self,
        _r: &ResourceRef,
        ctx: &AdapterCtx,
    ) -> Result<NormalizedCollection, AdapterError> {
        let v = ctx
            .tools
            .run_json(Tool::YtDlp, &["toy".into()], &ctx.cancel)
            .await?;
        let tracks = v["titles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| NormalizedTrack {
                platform_track_id: t.as_str().unwrap().into(),
                url: None,
                title: t.as_str().unwrap().into(),
                artists: vec!["Toy Artist".into()],
                album: None,
                track_number: None,
                duration_ms: None,
                isrc: None,
                artwork_url: None,
                youtube_video_id: None,
            })
            .collect();
        Ok(NormalizedCollection {
            platform_playlist_id: "1".into(),
            kind: ResourceKind::Playlist,
            name: "Toy".into(),
            owner_name: None,
            artwork_url: None,
            fetch_mode: FetchMode::Official,
            tracks,
            unavailable: vec![],
        })
    }
    async fn fetch_track(
        &self,
        _r: &ResourceRef,
        _ctx: &AdapterCtx,
    ) -> Result<NormalizedTrack, AdapterError> {
        Err(AdapterError::NotFound)
    }
}

async fn setup() -> (Arc<dyn PlatformAdapter>, AdapterCtx, impl Fn() -> usize, ()) {
    let dir = std::env::temp_dir().join(format!("toy-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("toy.json");
    std::fs::write(&file, r#"{"titles":["First","Middle","Last"]}"#).unwrap();
    let tools = Arc::new(FakeToolRunner::new().on(Tool::YtDlp, "toy", file));
    let counter = tools.clone();
    let ctx = crate::common::ctx(crate::common::test_net(), tools as Arc<dyn ToolRunner>);
    (
        Arc::new(ToyAdapter) as Arc<dyn PlatformAdapter>,
        ctx,
        move || counter.call_count(),
        (),
    )
}

crate::adapter_contract_suite!(
    setup(),
    cases: [CollectionCase {
        url: "https://example.com/playlist/1",
        kind: ResourceKind::Playlist,
        expected_tracks: 3,
        expected_unavailable: 0,
        first_title: "First",
        last_title: "Last",
    }]
);
