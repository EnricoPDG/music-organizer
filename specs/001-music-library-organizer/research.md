# Research: Music Library Organizer

**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Date**: 2026-10-11

Versions below were checked against crates.io, npm and GitHub releases on 2026-10-11.

## R1. Desktop framework ("best tech stack for multiple OS, beautiful and smooth")

- **Decision**: **Tauri 2** (2.12.x). The backend is Rust and the UI is a web frontend rendered in the
  OS's native webview.
- **Rationale**:
  - It produces installers for Windows (MSI/NSIS), macOS (DMG/app) and Linux (AppImage/deb/rpm)
    from one codebase, which matches the constitution's Windows/Linux/macOS requirement.
  - Installers are about 5–15 MB and idle memory is low, because it uses the system webview instead
    of bundling Chromium. Startup is fast (SC-007).
  - A web UI gives the most freedom for a polished design and smooth animations (FR-026).
  - Rust handles the heavy work (scanning, tagging, a download queue with concurrency limits) off
    the UI thread, so the UI stays responsive (FR-027).
  - It has first-party plugins for things this app needs: dialogs, opener (reveal in folder), shell
    (spawning sidecar processes), deep links (OAuth callback) and the updater.
- **Alternatives considered**:
  - **Electron**: the most consistent rendering across systems and the largest ecosystem, but it
    ships at about 100+ MB per installer with much higher memory use. Rejected for weight; the
    rendering consistency gain does not justify it here.
  - **Wails v2/v3 (Go)**: similar model to Tauri. Its plugin ecosystem is smaller, v3 was still in
    alpha/beta for most of 2025–26, and it has no built-in updater or deep-link support comparable
    to Tauri's.
  - **Flutter desktop**: smooth custom rendering, but the desktop integrations needed here (system
    trash, credential store, file-manager reveal) are weaker. Dart has no mature audio-tagging
    library.
  - **Native per OS (SwiftUI/WinUI/GTK)**: three separate UIs, which contradicts constitution
    Principle VI.
- **Known risk**: Linux uses WebKitGTK (webkit2gtk-4.1), which is the slowest of the three
  webviews for heavy CSS effects. Mitigation: GPU-cheap animations only (`transform`/`opacity`),
  virtualized lists, a "reduce motion" setting, and performance checks on Linux in CI and in the
  quickstart.

## R2. Frontend stack

- **Decision**:
  - **Svelte 5** (runes) with **SvelteKit** in static/SPA mode (`adapter-static`, SSR off). Tauri
    requires no server-side rendering.
  - **Tailwind CSS 4** for styling, with design tokens defined as CSS variables for the light and dark
    themes.
  - **Bits UI** for accessible headless components (dialogs, menus, comboboxes).
  - **@tanstack/svelte-virtual** for lists of 10k+ tracks (SC-006).
  - Svelte's built-in transitions for animation, plus the View Transitions API where the webview
    supports it.
- **Rationale**: Svelte compiles to small, fast code with no virtual-DOM overhead, which helps most
  on WebKitGTK. Runes make shared state for the download queue simple.
- **Alternatives considered**: React with shadcn/ui has a larger ecosystem but more runtime overhead
  and boilerplate. SolidJS is fast but has a smaller component ecosystem.

## R3. Rust ↔ UI contract

- **Decision**: Tauri commands for requests and Tauri events for progress streams. TypeScript
  bindings are generated from Rust types with **tauri-specta / specta 2.0.0-rc.25**, pinned to an
  exact version.
- **Rationale**: A single source of truth for the contract ([contracts/ipc.md](./contracts/ipc.md)).
  If the Rust and TypeScript sides drift apart, the build fails.
- **Alternatives considered**: hand-written TypeScript types (they drift silently) or `ts-rs` (types
  only, no command wrappers). The release-candidate status is acceptable: the library has been
  widely used in production on Tauri 2 since 2024. Pinning plus a binding-generation check in CI
  limits the risk. Fallback: hand-written bindings checked by the same CI step.

## R4. Audio source & downloading (YouTube)

- **Decision**: Use **yt-dlp** (latest release 2026.08.19) as an external tool for YouTube search,
  YouTube and SoundCloud playlist metadata, and audio download. Alongside it:
  - **Deno** (v2.9.x), the JavaScript runtime that yt-dlp has required for YouTube since 2025.11;
  - **FFmpeg**, for remuxing and the optional MP3 conversion.
  The app does **not** bundle these tools in the installer. A **Tool Manager** downloads them on
  first run into the app data folder, verifies their checksums, and updates them (yt-dlp weekly or
  on failure).
- **Rationale**:
  - yt-dlp is the only reliably maintained YouTube extractor. Reimplementing it, as v1 did, breaks
    whenever YouTube changes.
  - Since November 2025, yt-dlp needs an external JavaScript runtime for full YouTube support. Deno
    is the only runtime enabled by default.
  - YouTube changes often enough that yt-dlp must be updated faster than app releases, so the tools
    are managed separately from the installer.
  - Downloading the tools on first run keeps the installer small.
- **Alternatives considered**:
  - **Bundling the tools as Tauri sidecars**: adds about 150 MB to the installer and freezes the
    yt-dlp version until the next app release. Rejected.
  - **Rust-native extractors** (e.g. `rusty_ytdl`): they lag behind YouTube changes.
  - **Relying on tools the user installed themselves**: poor experience, inconsistent versions.
  - **Exception**: if the user sets a custom tool path, the app uses it (supports
    distro-packaged installs on Linux).
- **Licensing**: yt-dlp is under the Unlicense and Deno under MIT. FFmpeg uses LGPL builds, so the
  app can use them without being bound by the GPL.

## R5. Audio format & tagging

- **Decision**:
  - Default format is **M4A (AAC)**, taking YouTube's AAC stream without re-encoding.
  - **Opus** is also offered, without re-encoding.
  - **MP3** is available through FFmpeg conversion (this re-encodes and loses some quality, and the
    UI says so).
  - Tags are written with **lofty 0.25** (MP4 atoms, Vorbis comments, ID3v2.4): title, artists,
    album, album artist, track number, year, cover art, and a custom source link and ID for
    deduplication.
- **Rationale**: M4A has the widest player support and needs no quality loss, which meets FR-010's
  "keep original quality" default. lofty reads and writes every format the library scan will
  encounter (MP3, M4A, FLAC, Opus/Ogg, WAV), all in Rust.
- **Cover art**: use the platform artwork (Spotify album art, SoundCloud artwork). For YouTube-only
  tracks, use the video thumbnail cropped to a square.

## R6. Platform adapters (constitution Principle I)

All adapters implement the same `PlatformAdapter` contract
([contracts/platform-adapter.md](./contracts/platform-adapter.md)).

| Platform | Public link (no sign-in) | Signed-in (private playlists, Liked songs) |
|----------|--------------------------|--------------------------------------------|
| YouTube | yt-dlp `--flat-playlist -J` | YouTube Data API v3, OAuth (desktop client, PKCE, loopback redirect) |
| SoundCloud | yt-dlp (sets, tracks, users) | Official API, OAuth 2.1 + PKCE, using the user's own app (needs Artist Pro) |
| Spotify | Official API for metadata. **Track list** for playlists the user doesn't own: public embed page (best-effort, isolated in the adapter) | Official Web API, OAuth PKCE (no client secret), using the user's own developer app |

- **Spotify findings (February 2026 migration guide)**:
  - The playlist contents endpoint is now `GET /playlists/{id}/items`. It works only for playlists
    the user owns or collaborates on; other playlists return metadata without `items`.
  - The batch `GET /tracks` and `GET /albums` endpoints were removed. Single-item lookups remain.
  - Development Mode requires the app owner to have Premium and allows at most 5 users.
  - The `external_ids` field (which contains the ISRC) was restored in March 2026. It is useful for
    matching.
  - **Decision (agreed with the user)**: use the official API for signed-in access. For other
    people's public playlists, fall back to the embed page, marked as "best-effort".
- **Credentials model ("bring your own app")**: for each platform, the user pastes a client ID
  (plus a secret where the platform requires one) from their own developer app. A guided
  in-app setup walks them through it (FR-004c). No credentials are shipped with the app, so the
  app is not bound by per-app user limits, and the constitution's local-first rule holds.
- **OAuth redirect**: a loopback redirect to `http://127.0.0.1:<random port>/callback`. The app
  runs a temporary local HTTP listener in Rust and opens the system browser. This is supported by
  Google, Spotify (loopback IP literal) and SoundCloud, and needs no custom URL scheme
  registration on each system.
- **Known constraint**: Google OAuth apps in "testing" status issue refresh tokens for sensitive
  scopes such as `youtube.readonly` that expire after 7 days. The app detects an expired sign-in
  and asks the user to reconnect (spec edge case); public playlists keep working.

## R7. Matching algorithm (non-YouTube → YouTube)

- **Decision**: score up to 10 candidates from a yt-dlp `ytsearch10:"<artists> - <title>"` search.
  - **Signals**:
    - ISRC match (when available) in the description or YouTube Music metadata: strong boost;
    - normalized title similarity (Jaro-Winkler via `strsim`);
    - artist match in the title or channel;
    - duration difference (≤ 3 s strong, ≤ 10 s weak, > 30 s rejected);
    - boost for official artist and "- Topic" channels;
    - penalty for "live", "cover", "remix", "sped up", "8D", "karaoke" and "instrumental" unless
      the source title contains the same word.
  - **Confidence**: High ≥ 0.85, Medium ≥ 0.65, Low below 0.65. All candidates are kept for review
    (FR-006, FR-007).
- **Rationale**: deterministic and explainable (Principle IV requires recording how a match was
  chosen). It can be tested with recorded search fixtures (Principle V). It also gives SC-002's 90%
  target a measurable scorer: a labelled fixture set of 200+ tracks is evaluated in CI.
- **Alternatives**: audio fingerprinting (AcoustID) needs a download before matching and an
  external service. It is deferred.

## R8. Storage & search

- **Decision**: **SQLite** through **rusqlite 0.40** (`bundled` feature, so the same SQLite version
  on every system), in WAL mode, with **FTS5** for search. Database migrations are embedded in the
  binary.
- **Rationale**:
  - The data lives in one inspectable local file, meeting the constitution's local-first rule.
  - FTS5 prefix search over 10k tracks responds in well under 50 ms (SC-006).
  - Synchronous rusqlite on a dedicated database thread is simpler than async `sqlx` and enough for
    a single-user app.
- **Alternatives**: `sqlx` (async, compile-time-checked queries) brings complexity that isn't
  needed. JSON files can't search 10k tracks fast or keep data consistent.

## R9. Library scanning & file watching

- **Decision**: **walkdir** to discover files, then tag reads with **lofty** in a bounded worker pool,
  writing rows to SQLite in batches of 500. **notify 8** watches folders for changes, debounced, with
  a full rescan on app start (FR-019). Scanned files are opened **read-only** (FR-015).
- **Rationale**: Meets SC-010 (5,000 files in under 2 minutes). Tag reading is I/O-bound and runs at
  about 200–1,000 files per second on SSDs.

## R10. File safety (constitution Principle III)

- **Decision**:
  - **Downloads**: write to `<music folder>/.music-organizer/tmp/<job>.part`, tag the file there,
    then rename it into place atomically on the same volume. On startup, delete leftover `.part`
    files.
  - **Renames and moves**: a plan → preview → apply flow (`OrganizePlan`). Each applied plan writes
    an undo journal.
  - **Deletion**: the `trash` crate (5.2), which uses the system trash or recycle bin.
  - **File names**: one sanitizer for all systems. It removes `<>:"/\|?*` and control characters,
    handles Windows reserved names (`CON`, `NUL`, …) and trailing dots or spaces, and limits
    components to 120 characters and total paths to 240 characters on Windows.
- **Rationale**: Meets SC-009 and the cross-platform edge cases in the spec.

## R11. Credentials

- **Decision**: **keyring 4.2**. It uses the macOS Keychain, Windows Credential Manager, and Linux
  Secret Service through D-Bus. The database stores only a key reference. Tokens are never logged:
  they are wrapped in a `Secret<T>` type whose debug output is redacted.
- **Linux fallback**: if no Secret Service is available (minimal window managers), the app reports
  it and refuses to store tokens in plaintext. Public-link features keep working.

## R12. Networking resilience (constitution Principle IV)

- **Decision**: **reqwest 0.13** + **tokio**.
  - Each platform has its own rate limiter (**governor**).
  - Retries back off exponentially with jitter (base 1 s, maximum 5 attempts) and honor
    `Retry-After` on 429 responses (Spotify's 429 `QUOTA_EXCEEDED`).
  - The download queue allows at most 3 downloads at once, plus 1–3 s of jitter between YouTube
    requests.
  - Each track's state is persisted, so the app can resume after a crash or exit.

## R13. Testing & CI

- **Decision**:
  - **Rust**: `cargo test`, using fixture JSON and HTML recorded from each platform and yt-dlp
    output under `src-tauri/tests/fixtures/`. A fake tool runner replaces the real yt-dlp process.
    There are adapter contract tests (Principle V) and a matcher accuracy test over the labelled set.
  - **Frontend**: **Vitest 5** for stores and utilities. **Playwright 1.64** runs the built frontend
    against mocked Tauri IPC (`@tauri-apps/api/mocks`) for UI flows.
  - **Live checks**: a separate `cargo test --features live -- --ignored` suite. It does not run by
    default.
  - **CI**: GitHub Actions matrix on `ubuntu-latest`, `windows-latest` and `macos-latest`, running
    lint, tests and `tauri build`.
- **Rationale**: Real-app WebDriver end-to-end tests (`tauri-driver`) don't support macOS, so the
  real-app check is the manual [quickstart.md](./quickstart.md) on all three systems, as the
  constitution's Definition of Done requires.

## Sources

- [Spotify Feb 2026 Web API migration guide](https://developer.spotify.com/documentation/web-api/tutorials/february-2026-migration-guide)
- [TechCrunch: Spotify changes developer mode API](https://techcrunch.com/2026/02/06/spotify-changes-developer-mode-api-to-require-premium-accounts-limits-test-users/)
- [Vorp Labs: Spotify API changes 2026](https://vorplabs.com/agent-tools/spotify-api-changes)
- [yt-dlp issue #15012: external JS runtime requirement](https://github.com/yt-dlp/yt-dlp/issues/15012)
- [SoundCloud API docs](https://developers.soundcloud.com/docs/api) ·
  [OAuth 2.1 migration](https://developers.soundcloud.com/blog/oauth-migration)
- [Tauri runtime-wry 2.10.0 notes](https://v2.tauri.app/release/tauri-runtime-wry/v2.10.0/) ·
  [webkit2gtk-4.1 migration](https://v2.tauri.app/blog/tauri-2-0-0-alpha-3/)
