---

description: "Task list for the Music Library Organizer feature"
---

# Tasks: Music Library Organizer

**Input**: Design documents from `/specs/001-music-library-organizer/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/](./contracts/), [quickstart.md](./quickstart.md)

**Tests**: INCLUDED. The constitution requires them: Principle V says "Core logic … MUST be developed
test-first" and "Every adapter MUST have contract tests". Tests never call live platform APIs; they
use fixtures under `src-tauri/tests/fixtures/` and the `FakeToolRunner`.

**Organization**: Tasks are grouped by user story, so each story can be built and tested on its own.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on unfinished tasks)
- **[Story]**: The user story the task belongs to (US1–US5)
- Paths follow the plan's single Tauri project: `src/` (Svelte UI), `src-tauri/` (Rust core), `e2e/`

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization and tooling

- [X] T001 Scaffold the Tauri 2 + SvelteKit (TypeScript) app at the repository root with `pnpm create tauri-app`. Keep the `src/` and `src-tauri/` layout from plan.md, and set the app identifier `dev.musicorganizer.app` and product name "Music Organizer" in `src-tauri/tauri.conf.json`
- [X] T002 Configure SvelteKit for SPA mode: `@sveltejs/adapter-static` with `fallback: "index.html"` passed to `sveltekit()` in `vite.config.ts` (SvelteKit 3 no longer reads `svelte.config.js`), and `export const ssr = false; export const prerender = false;` in `src/routes/+layout.ts`
- [X] T003 Add the Rust dependencies to `src-tauri/Cargo.toml`, with exact pins where the plan says so:
  - tauri 2.12, tauri-plugin-dialog, tauri-plugin-opener, tauri-plugin-shell, tauri-plugin-updater
  - `tauri-specta = "=2.0.0-rc.25"`, `specta = "=2.0.0-rc.25"`, specta-typescript
  - tokio 1.53 (full), reqwest 0.13 (rustls, json, stream), `rusqlite 0.40` (features `bundled`)
  - lofty 0.25, keyring 4.2, trash 5.2, notify 8.2, walkdir 2.5, governor 0.10, oauth2 5, strsim 0.11
  - uuid (v7), serde/serde_json, thiserror, tracing, tracing-subscriber, tracing-appender, url,
    sha2, unicode-normalization, async-trait, tokio-util
- [X] T004 [P] Add the frontend dependencies to `package.json`: svelte 5.57, @sveltejs/kit 3, tailwindcss 4.3 + @tailwindcss/vite, bits-ui 2.19, @tanstack/svelte-virtual 3.13, @tauri-apps/api 2.12 and the matching plugin packages (dialog, opener, shell, updater), plus vitest 5, @playwright/test 1.64 and svelte-check
- [X] T005 [P] Configure linting and formatting:
  - `src-tauri/rustfmt.toml`;
  - `src-tauri/.cargo/config.toml` with clippy `-D warnings` in CI;
  - `eslint.config.js` and `.prettierrc` (with prettier-plugin-svelte);
  - `package.json` scripts `lint`, `format`, `check`, `test`, `test:e2e`
- [X] T006 [P] Configure Vitest in `vite.config.ts` (`test.include: ["src/**/*.test.ts"]`, jsdom environment) and Playwright in `playwright.config.ts` (webServer = `pnpm vite preview`, baseURL, chromium + webkit projects)
- [X] T007 [P] Create the CI workflow `.github/workflows/ci.yml`:
  - matrix `ubuntu-22.04`, `windows-latest`, `macos-latest`;
  - install the Linux packages `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libsecret-1-dev`;
  - run `pnpm lint`, `pnpm check`, `pnpm test`, `pnpm test:e2e`, `cargo clippy`, `cargo test` and `pnpm tauri build --debug`;
  - no network access is needed by the tests
- [X] T008 [P] Update the root `.gitignore`: add `node_modules/`, `build/`, `.svelte-kit/`, `src-tauri/target/`, `src-tauri/gen/`, `test-results/`, `playwright-report/`
- [X] T009 Create the Rust module skeleton from plan.md: empty `mod.rs` files for `src-tauri/src/{commands,core,platforms,tools,net,db,os}/`, declared in `src-tauri/src/lib.rs`, plus an empty `src-tauri/tests/fixtures/{spotify,youtube,soundcloud,ytdlp}/` tree with `.gitkeep`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure that every user story depends on

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

### Tests first (must FAIL before implementation)

- [X] T010 [P] Write sanitizer tests in `src-tauri/tests/sanitize.rs`:
  - remove `<>:"/\|?*` and control characters;
  - rename Windows reserved names `CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9` (e.g. `CON` → `CON_`);
  - strip trailing dots and spaces;
  - limit each path component to 120 characters, cut on a UTF-8 character boundary;
  - limit the total path to 240 characters on Windows;
  - identical output on every OS
- [X] T011 [P] Write normalization and dedup-key tests in `src-tauri/tests/dedup.rs`:
  - `norm()` lower-cases the text, strips diacritics and punctuation, and removes "feat." clauses and "(Official Video)"-style suffixes;
  - `dedup_key = norm(primary artist) + "|" + norm(title)`; durations are compared separately (±3 s), because a fixed bucket can split two durations 2 s apart;
  - the same song with a ±2 s duration difference matches
- [X] T012 [P] Write net retry and rate-limit tests in `src-tauri/tests/net.rs`, using a local mock HTTP server:
  - retries with exponential backoff and jitter (base 1 s, maximum 5 attempts);
  - honors `Retry-After` on 429 responses;
  - `QUOTA_EXCEEDED` is treated as rate limiting;
  - timeouts give a `Network` error
- [X] T013 [P] Write DB migration tests in `src-tauri/tests/db.rs`:
  - the migrations apply to an empty database;
  - they are idempotent when run again;
  - FTS5 triggers keep `track_fts` in sync on insert, update and delete;
  - the unique constraints exist: `Track.file_path`, `SourceRef(platform, platform_track_id)`, `Playlist(platform, platform_playlist_id)`, `WatchedFolder.path`
- [X] T014 [P] Write `Secret<T>` tests in `src-tauri/tests/secrets.rs`: the `Debug` and `Display` output is `"[REDACTED]"`, and serializing to logs never includes the inner value

### Implementation

- [X] T015 Implement `AppError { kind: ErrorKind, message: String, retryable: bool }` and the `From` conversions in `src-tauri/src/error.rs`. Its specta and serde derivations must match [contracts/ipc.md](./contracts/ipc.md). Messages must be safe to show to the user and never contain secrets
- [X] T016 [P] Implement `sanitize_component()` and `build_path()` in `src-tauri/src/core/sanitize.rs` so that T010 passes
- [X] T017 [P] Implement `norm()` and `dedup_key()` in `src-tauri/src/core/model.rs` so that T011 passes. Define the core types in the same file:
  - `NormalizedTrack`, `NormalizedCollection`, `ResourceRef`, `ResourceKind`, `FetchMode`, `UnavailableItem`, as in [contracts/platform-adapter.md](./contracts/platform-adapter.md);
  - the id newtypes, which are UUIDv7 values
- [X] T018 [P] Implement `Secret<T>` and the keyring wrapper (`store`, `load`, `delete` under service `music-organizer`) in `src-tauri/src/secrets.rs`. When no Secret Service is available on Linux, return `ErrorKind::CredentialStoreUnavailable`; never fall back to plaintext (research R11)
- [X] T019 Implement the DB layer in `src-tauri/src/db/mod.rs`: a dedicated connection thread, WAL mode, `foreign_keys=ON`, a migration runner, and async `db.call(|conn| …)` helpers
- [X] T020 Write the migration `src-tauri/src/db/migrations/0001_init.sql` with every table from [data-model.md](./data-model.md), using these constraints:
  - **Track**:
    - `title` "required, 1–500 chars";
    - `artists` JSON array ("≥ 1 entry");
    - `track_number` "≥ 1";
    - `year` "1000–2100";
    - `duration_ms` "> 0";
    - `isrc` `^[A-Z]{2}[A-Z0-9]{3}\d{7}$`, enforced in code;
    - `file_format` in (`m4a`, `opus`, `mp3`, `flac`, `ogg`, `wav`);
    - `origin` in (`downloaded`, `scanned`);
    - `status` in (`pending`, `available`, `missing`, `failed`);
    - `dedup_key` indexed.
  - **SourceRef**: unique `(platform, platform_track_id)`, with cascade on track delete.
  - **Match**: `confidence` 0–1; `confidence_level` in (`high`, `medium`, `low`); `method` in (`direct`, `auto`, `user_candidate`, `user_url`).
  - **MatchCandidate**: `rank`.
  - **Playlist**: `kind` in (`imported`, `local`); `fetch_mode` in (`official`, `public_fallback`).
  - **PlaylistEntry**: `position` dense from 0; `removed_online_at`.
  - **ImportBatch** and **DownloadJob**: `progress` 0–1; `attempts` "≤ 5 automatic retries"; the `error_kind` enum from data-model.md; a partial unique index on active `track_id`.
  - **ConnectedAccount**: `status` in (`connected`, `expired`, `error`).
  - **WatchedFolder**: unique `path`, and a partial unique index so exactly one row has `is_download_target`.
  - **OrganizePlan** and **OrganizeOp**: `conflict` in (`none`, `exists`, `invalid_name`, `too_long`).
  - **Settings** as key/value.
  - The `track_fts` FTS5 table with its triggers, and the `artist_view` and `album_view` SQL views.
  T013 must pass afterwards.
- [X] T021 [P] Implement the net layer in `src-tauri/src/net/mod.rs`: a shared reqwest client with timeouts, a governor rate limiter per platform, and a retry policy that honors `Retry-After`. T012 must pass
- [X] T022 [P] Implement the `ToolRunner` trait in `src-tauri/src/tools/mod.rs`. It runs a tool with arguments and gets JSON or streamed progress back, with cancellation. Also implement `FakeToolRunner`, which replays fixture files from `src-tauri/tests/fixtures/ytdlp/`, for tests
- [X] T023 Implement the `PlatformAdapter` trait in `src-tauri/src/platforms/mod.rs`, exactly as in [contracts/platform-adapter.md](./contracts/platform-adapter.md):
  - `Capabilities`, `AdapterError`, `AdapterCtx` (http, tools, token, cancel);
  - `registry()`, which returns `Vec<Arc<dyn PlatformAdapter>>`;
  - `resolve(url)`, which asks each adapter's `parse_url`
- [X] T024 Implement the shared `adapter_contract_suite!` macro in `src-tauri/tests/common/contract.rs`. It checks the contract's behavioral rules 1–7:
  - `parse_url` is pure and rejects other platforms' URLs;
  - `fetch_collection` returns all pages in order and puts unavailable items in `unavailable`;
  - every track has a non-empty title and ≥ 1 artist;
  - output is deterministic;
  - adapters never write anything;
  - all I/O goes through `ctx`;
  - a 429 becomes `RateLimited`
- [X] T025 [P] Implement the OS helpers in `src-tauri/src/os/mod.rs`: app data, log and cache directories; `reveal_in_folder` and `open_with_default` through tauri-plugin-opener; and the system reduced-motion and theme queries
- [X] T026 [P] Set up logging in `src-tauri/src/logging.rs`: tracing with a rolling file appender in the app log directory, plus a redaction layer that masks `access_token`, `refresh_token`, `client_secret`, `code` and `Authorization` values
- [X] T027 Implement the settings repository and commands `get_settings` and `update_settings` in `src-tauri/src/commands/settings.rs`, with the defaults from data-model.md:
  - `audio_format` = `m4a`;
  - `naming_pattern` = `{album_artist}/{album}/{track:02} - {title}`;
  - `theme` = `system`;
  - `reduce_motion` follows the OS;
  - `max_parallel_downloads` = `3`, "(1–5)";
  - `review_low_confidence` = `true`
- [X] T028 Wire up `src-tauri/src/lib.rs`: plugins, managed state (Db, registry, net, tools, queue placeholder), and tauri-specta command and event collection. In debug builds, export the TypeScript bindings to `src/lib/bindings.ts`. Add the script `pnpm bindings` and a CI step that fails if `git diff --exit-code src/lib/bindings.ts` shows changes
- [X] T029 [P] Build the frontend theme and design tokens in `src/app.css`:
  - Tailwind 4 `@theme` with CSS variables for the light and dark palettes;
  - the type scale and radii;
  - a `prefers-reduced-motion` and `.reduce-motion` override;
  - animations limited to `transform` and `opacity` (research R1, Linux WebKitGTK)
- [X] T030 [P] Create the base UI components in `src/lib/components/`, built on Bits UI where it applies: `Button.svelte`, `Badge.svelte` (high/medium/low variants), `Dialog.svelte`, `Toast.svelte`, `ProgressBar.svelte`, `EmptyState.svelte`, `ArtworkImg.svelte` (lazy, with a fade-in)
- [X] T031 Create the app shell `src/routes/+layout.svelte`: a sidebar with Import, Queue, Review, Library, Organize and Settings; theme handling (system, light or dark, from settings); the toast host; and route transitions that respect reduce motion
- [X] T032 [P] Create the typed API layer `src/lib/api/index.ts`, which re-exports the generated commands and maps `AppError` to toasts. Add `src/lib/api/events.ts`, with typed subscribe helpers for every event in contracts/ipc.md that unsubscribe automatically when a component is destroyed
- [X] T033 Implement the first-run onboarding and music-folder selection. It adds a `WatchedFolder` with `is_download_target = true` through `add_watched_folder`, which is implemented minimally in `src-tauri/src/commands/library.rs`. The route is `src/routes/onboarding/+page.svelte`, and the layout redirects there when no download target exists

**Checkpoint**: the foundation is ready; user stories can start

---

## Phase 3: User Story 1 - Import a playlist and download it (Priority: P1) 🎯 MVP

**Goal**: paste a Spotify, YouTube or SoundCloud link (or connect an account), auto-match to YouTube, and download tagged files safely, with live per-track status

**Independent Test**: [quickstart.md](./quickstart.md) V1, V2, V3 steps 1–3, V4 and V5

### Tests for User Story 1 (write first, must FAIL) ⚠️

- [ ] T034 [P] [US1] Record the YouTube fixtures in `src-tauri/tests/fixtures/youtube/` (yt-dlp `--flat-playlist -J` output for a playlist, a single video, a playlist with a private or removed entry, and a `music.youtube.com` link). Write the contract test `src-tauri/tests/contract_youtube.rs` using `adapter_contract_suite!`
- [ ] T035 [P] [US1] Record the Spotify fixtures in `src-tauri/tests/fixtures/spotify/`:
  - Web API JSON for an owned playlist (`GET /playlists/{id}/items`, two pages), a non-owned playlist (metadata only, without `items`), `GET /tracks/{id}`, an album, and `/me/library` liked tracks;
  - the public embed page HTML for a non-owned playlist;
  - a changed embed page that must give `PublicSourceChanged`;
  - a 429 response with `Retry-After`.
  Write the contract test `src-tauri/tests/contract_spotify.rs`, which also asserts the correct `fetch_mode`
- [ ] T036 [P] [US1] Record the SoundCloud fixtures in `src-tauri/tests/fixtures/soundcloud/` (yt-dlp JSON for a set and a track, an `on.soundcloud.com` short link, and API JSON for `/me/likes` and `/me/playlists`). Write the contract test `src-tauri/tests/contract_soundcloud.rs`
- [ ] T037 [P] [US1] Write matcher tests in `src-tauri/tests/matcher.rs`. Cover the scoring signals from research R7:
  - an ISRC match gives a strong boost;
  - title similarity uses Jaro-Winkler;
  - an artist found in the title or channel counts;
  - duration "≤ 3 s strong, ≤ 10 s weak, > 30 s rejected";
  - official and "- Topic" channels get a boost;
  - "live/cover/remix/sped up/8D/karaoke/instrumental" are penalized unless the source title contains them.
  Also test the levels: `high ≥ 0.85`, `medium ≥ 0.65`, `low` otherwise; and that `signals` explains the score
- [ ] T038 [P] [US1] Write the matcher accuracy test in `src-tauri/tests/matcher_accuracy.rs`. It uses a labelled set of ≥ 200 tracks (`src-tauri/tests/fixtures/ytdlp/match_set/*.json`, each with recorded `ytsearch10` results and the correct video id), asserts ≥ 90% correct (SC-002), and prints a per-case report on failure
- [ ] T039 [P] [US1] Write importer tests in `src-tauri/tests/importer.rs`, using the fake adapter and `FakeToolRunner`:
  - importing creates a Playlist, tracks, `SourceRef`s, matches and jobs;
  - importing the same playlist again creates no new tracks or files, and its jobs become `skipped` (FR-014, SC-005);
  - the same song in two playlists creates one track and two `PlaylistEntry` rows;
  - YouTube tracks get `method = direct`, `confidence = 1`
- [ ] T040 [P] [US1] Write download queue tests in `src-tauri/tests/queue.rs` for the full state machine in data-model.md:
  - `queued → matching → downloading → tagging → finalizing → done`;
  - pause and resume; cancel deletes the `.part` file;
  - automatic retries "≤ 5"; then `failed` with an `error_kind`;
  - one failing job does not stop the others (FR-012);
  - concurrency never exceeds `max_parallel_downloads`;
  - on startup, jobs in `downloading/tagging/finalizing` go back to `queued` and leftover `.part` files are deleted;
  - progress events are throttled to ≤ 10 per second
- [ ] T041 [P] [US1] Write tagger tests in `src-tauri/tests/tagger.rs`. For each of `m4a`, `opus` and `mp3` sample files in `src-tauri/tests/fixtures/audio/`, write and then read back: title, artists, album, album artist, track number, year, cover art, and the custom source URL and ID
- [ ] T042 [P] [US1] Write the atomic finalize test in `src-tauri/tests/finalize.rs`:
  - the download goes to `<music folder>/.music-organizer/tmp/<job>.part`;
  - the final path comes from the naming pattern and `sanitize`;
  - no file appears at the final path until the rename;
  - name collisions get ` (2)` added, never an overwrite
- [ ] T043 [P] [US1] Write OAuth loopback tests in `src-tauri/tests/oauth.rs`:
  - the PKCE verifier and challenge are generated;
  - the listener binds to `127.0.0.1` on a random port;
  - the state is validated, and a mismatch is rejected;
  - the listener times out after 5 minutes;
  - tokens are stored only through `secrets.rs`
- [ ] T044 [P] [US1] Write the Playwright import flow `e2e/import.spec.ts`, using mocked IPC (`@tauri-apps/api/mocks`):
  - paste a link, see the preview card, start the import;
  - the queue rows advance as mocked `job://progress` events arrive;
  - a failed row shows its reason and a Retry action;
  - a `private` error suggests signing in

### Implementation for User Story 1

- [ ] T045 [US1] Implement the tool manager in `src-tauri/src/tools/manager.rs`:
  - install yt-dlp (≥ 2026.08.19), Deno 2.9 and an LGPL FFmpeg build for the current OS and architecture into `<app data>/tools/`;
  - verify SHA-256 checksums against a pinned manifest `src-tauri/src/tools/manifest.json`;
  - update yt-dlp weekly or after an extractor failure;
  - let the user override a tool's path;
  - emit `tools://status`.
  Also add the commands `tools_status` and `install_tools` in `src-tauri/src/commands/tools.rs`
- [ ] T046 [US1] Implement the real yt-dlp `ToolRunner` in `src-tauri/src/tools/ytdlp.rs`:
  - pass `--js-runtimes deno:<path>` and `--ffmpeg-location`;
  - build the commands for flat playlist JSON, `ytsearch10:` JSON, and audio download in the format chosen in settings (m4a: `-f ba[ext=m4a]`, no re-encode; opus: `-f ba[acodec=opus]`, remuxed; mp3: `-x --audio-format mp3`);
  - parse `--progress-template` output into progress values
- [ ] T047 [P] [US1] Implement the YouTube adapter in `src-tauri/src/platforms/youtube.rs`. Public links go through yt-dlp. Sign-in uses the YouTube Data API v3 (`playlists?mine=true`, the `LL` liked playlist, `playlistItems`). URL parsing covers `youtube.com/playlist`, `watch?v=&list=`, `youtu.be`, `music.youtube.com` and `shorts`. `provides_audio = true`, and `youtube_video_id` is set on each track. T034 must pass
- [ ] T048 [P] [US1] Implement the Spotify adapter in `src-tauri/src/platforms/spotify.rs`:
  - the official Web API (PKCE, no client secret): `GET /playlists/{id}` metadata, `GET /playlists/{id}/items` with paging when the user owns or collaborates on the playlist, single `GET /tracks/{id}` and `GET /albums/{id}`, and `/me/library` for liked tracks;
  - `isrc` from `external_ids`;
  - for non-owned public playlists, the embed-page fallback (`fetch_mode = PublicFallback`), returning `PublicSourceChanged` when parsing fails.
  T035 must pass
- [ ] T049 [P] [US1] Implement the SoundCloud adapter in `src-tauri/src/platforms/soundcloud.rs`. Public sets and tracks go through yt-dlp, and `on.soundcloud.com` short links are resolved. Sign-in uses the official API with OAuth 2.1 + PKCE (`/me/playlists`, `/me/likes/tracks`), and the single-use refresh tokens are rotated in the keyring. T036 must pass
- [ ] T050 [US1] Register the three adapters in `platforms::registry()` in `src-tauri/src/platforms/mod.rs`
- [ ] T051 [US1] Implement the matcher in `src-tauri/src/core/matcher.rs`. It builds the query `"<artists> - <title>"`, runs `ytsearch10:` through the ToolRunner, scores the candidates as in R7, and stores the `Match` (with `signals` JSON) and the top `MatchCandidate` rows (rank 1 = best, keeping ≥ 3 when available). It must never overwrite a `method` of `user_candidate` or `user_url`. T037 and T038 must pass
- [ ] T052 [US1] Implement the tagger in `src-tauri/src/core/tagger.rs`: lofty write for MP4 atoms, Vorbis comments and ID3v2.4, including artwork; an artwork cache by SHA-256 in `<app cache>/artwork/`; and square-cropped YouTube thumbnails when there is no album art. T041 must pass
- [ ] T053 [US1] Implement the importer in `src-tauri/src/core/importer.rs`:
  - `NormalizedCollection` → upsert the Playlist (`kind = imported`);
  - for each track, look up the `SourceRef` and then the `dedup_key` (which also finds scanned files, FR-015b); only then create a Track (`origin = downloaded`, `status = pending`);
  - write `PlaylistEntry` positions;
  - create an `ImportBatch` and `DownloadJob`s, marking tracks already `available` as `skipped`.
  T039 must pass
- [ ] T054 [US1] Implement the download queue in `src-tauri/src/core/queue.rs`:
  - a tokio worker pool bounded by `max_parallel_downloads`, with 1–3 s jitter between YouTube requests;
  - the state machine from data-model.md, with every transition saved to the database;
  - `.part` temp files → tag → atomic rename into the path from the naming pattern (sanitized);
  - pause, resume, cancel and retry;
  - resume and clean up on startup;
  - pause on network loss with a "waiting for network" status;
  - throttled `job://progress` and `batch://summary` events.
  T040 and T042 must pass
- [ ] T055 [US1] Implement the import commands `resolve_link`, `start_import` and `list_my_collections` in `src-tauri/src/commands/import.rs`, and the job commands `list_jobs`, `pause_jobs`, `resume_jobs`, `cancel_jobs` and `retry_jobs` in `src-tauri/src/commands/jobs.rs`, as in [contracts/ipc.md](./contracts/ipc.md)
- [ ] T056 [US1] Implement the OAuth loopback flow in `src-tauri/src/net/oauth.rs` (oauth2 5, PKCE, a temporary `127.0.0.1` listener, opening the system browser, 5-minute timeout) so that T043 passes. Add the account commands `list_accounts`, `connect_account` and `disconnect_account` in `src-tauri/src/commands/accounts.rs`:
  - only a key reference is stored in `ConnectedAccount.keyring_key`;
  - disconnecting deletes the keyring entry and the database row (FR-004a);
  - when a token expires, set `status = expired` and emit `account://status`
- [ ] T057 [P] [US1] Build the queue state store `src/lib/stores/queue.svelte.ts` with Svelte 5 runes. It subscribes to `job://progress` and `batch://summary`, keeps jobs keyed by id, and batches UI updates with `requestAnimationFrame`
- [ ] T058 [US1] Build the Import screen `src/routes/import/+page.svelte`:
  - a link input with paste detection;
  - a preview card (artwork, name, owner, track count, and a "best-effort public import" label when `fetchMode = public_fallback`);
  - the **Import & download** button;
  - an error state that suggests signing in for `private` and explains `public_source_changed`;
  - a "My playlists / Liked songs" section for connected accounts
- [ ] T059 [US1] Build the Queue screen `src/routes/queue/+page.svelte`: batches with overall progress, a virtualized job list with state icons, per-track progress and error reasons, and pause, resume, cancel and retry for each job and each batch
- [ ] T060 [US1] Build the accounts settings and the guided "bring your own app" setup (FR-004c) in `src/routes/settings/accounts/+page.svelte`. Each platform gets a step-by-step dialog: create a developer app, set the redirect `http://127.0.0.1` (loopback), paste the client ID (and secret where needed), then connect. Show the connected and expired status with Reconnect and Disconnect, and note the Spotify Premium and SoundCloud Artist Pro requirements
- [ ] T061 [US1] Build the tools section `src/routes/settings/tools/+page.svelte` (versions, Update, custom path), and add a first-run tools install step with progress to `src/routes/onboarding/+page.svelte`

**Checkpoint**: US1 is fully usable on its own (MVP); validate with quickstart V1–V5

---

## Phase 4: User Story 2 - Review and fix YouTube matches (Priority: P2)

**Goal**: see each match's confidence and candidates, replace a match, and have the app remember the choice

**Independent Test**: [quickstart.md](./quickstart.md) V3 step 4

**Depends on**: US1 (matches exist only after an import)

### Tests for User Story 2 ⚠️

- [ ] T062 [P] [US2] Write match command tests in `src-tauri/tests/match_commands.rs`:
  - `get_match` returns the chosen match, `confidenceLevel`, `signals`, and candidates "≥ 3 if available";
  - `set_match` with `candidateVideoId` sets `method = user_candidate`, and with `youtubeUrl` (validated to an 11-character id) sets `method = user_url`;
  - if the track was already downloaded, a re-download job is queued and replaces the file atomically;
  - an automatic re-match on re-import never overrides a user choice
- [ ] T063 [P] [US2] Write the queue test for the review gate in `src-tauri/tests/queue.rs`: with `review_low_confidence = true`, `low` matches stop at `awaiting_review` and continue after `set_match` or an explicit "download anyway"
- [ ] T064 [P] [US2] Write the Playwright review flow `e2e/review.spec.ts`: low-confidence tracks are listed first, a track opens with its candidates, selecting one updates the badge, and pasting a YouTube URL works

### Implementation for User Story 2

- [ ] T065 [US2] Implement `get_match`, `set_match` and `search_candidates` (with a custom query) in `src-tauri/src/commands/matching.rs`, and the re-download path in `src-tauri/src/core/queue.rs`. T062 must pass
- [ ] T066 [US2] Add the `awaiting_review` state handling and the "download anyway" action to `src-tauri/src/core/queue.rs` and `src-tauri/src/commands/jobs.rs`. T063 must pass
- [ ] T067 [P] [US2] Build the Match review screen `src/routes/review/+page.svelte`: a list sorted low → medium → high with confidence badges; a detail panel with the source track (artwork, artists, duration), the chosen video and the candidates (title, channel, duration difference, score), an "Open on YouTube" link, a search box for custom queries, and a "Paste YouTube link" field
- [ ] T068 [US2] Add the low-confidence highlight and the review prompt before downloading to the Import preview (`src/routes/import/+page.svelte`), and a "Fix match" action on failed or low rows in the Queue (`src/routes/queue/+page.svelte`)

**Checkpoint**: US1 and US2 both work; validate with quickstart V3

---

## Phase 5: User Story 3 - Browse and search the library (Priority: P3)

**Goal**: a polished, fast library (tracks, artists, albums, playlists, search) that includes scanned folders

**Independent Test**: [quickstart.md](./quickstart.md) V6 and V7. This works without US1: add a folder of existing music

### Tests for User Story 3 ⚠️

- [ ] T069 [P] [US3] Write scanner tests in `src-tauri/tests/scanner.rs`:
  - a fixture folder with mixed `mp3/m4a/flac/opus/ogg/wav`, plus corrupt and unsupported files, gives tracks with `origin = scanned`; corrupt files are skipped and listed;
  - file mtimes and contents are unchanged afterwards (FR-015);
  - an external delete marks the track `missing`, and its reappearance restores it to `available` (FR-019);
  - removing a folder deletes only the database rows (FR-015a)
- [ ] T070 [P] [US3] Write library query tests in `src-tauri/tests/library.rs`:
  - `list_tracks`, `list_artists` and `list_albums` paging (`limit ≤ 500`), sorting and filters;
  - `search` prefix matching across title, artist and album with `limit ≤ 50`;
  - a performance assertion: `search` on 10k generated tracks takes < 100 ms (SC-006 budget)
- [ ] T071 [P] [US3] Write the Playwright library flow `e2e/library.spec.ts` with 10k mocked rows: virtual scrolling renders fewer than 100 DOM rows, ⌘/Ctrl+K search shows results while typing, the Artists → Artist → Albums drill-down works, and the theme toggle switches between dark and light

### Implementation for User Story 3

- [ ] T072 [US3] Implement the scanner in `src-tauri/src/core/scanner.rs`:
  - walkdir discovery;
  - a bounded worker pool for read-only lofty tag and artwork reads;
  - database inserts in batches of 500, linking to an existing track via `dedup_key` when possible;
  - `scan://progress` events, at most 4 per second;
  - a full rescan at startup;
  - a debounced notify watcher for each `WatchedFolder`.
  T069 must pass
- [ ] T073 [US3] Implement the library commands in `src-tauri/src/commands/library.rs`:
  - `list_tracks`, `list_artists`, `list_albums`, `search` (FTS5 prefix), `get_artwork` (resized 64/256/600 and cached, served through the asset protocol), `reveal_in_folder`, `open_with_default`;
  - `add_watched_folder` (path must "exist and be readable when added"), `remove_watched_folder`, `rescan`;
  - a `library://changed` event after writes.
  T070 must pass
- [ ] T074 [P] [US3] Build the library store `src/lib/stores/library.svelte.ts`: a paged fetch cache keyed by `(view, sort, filter, offset)`, invalidated on `library://changed`
- [ ] T075 [P] [US3] Build the reusable virtualized `src/lib/components/TrackList.svelte` (@tanstack/svelte-virtual, fixed row height, keyboard navigation, multi-select, context menu with Show in folder, Open, Edit tags and Add to playlist), plus `src/lib/components/AlbumGrid.svelte` (a virtualized artwork grid)
- [ ] T076 [US3] Build the library routes:
  - `src/routes/library/+page.svelte` with tabs for Tracks, Artists, Albums and Playlists;
  - `src/routes/library/artist/[name]/+page.svelte`;
  - `src/routes/library/album/[id]/+page.svelte`.
  Include artwork headers, shared-element transitions where the View Transitions API is available, and empty states
- [ ] T077 [US3] Build the global search palette `src/lib/components/SearchPalette.svelte` (⌘/Ctrl+K, debounced 50 ms, grouped results for tracks, artists and albums, keyboard navigation) and mount it in `src/routes/+layout.svelte`
- [ ] T078 [US3] Build the folders settings `src/routes/settings/folders/+page.svelte`: add and remove watched folders (with a confirmation saying the files will be kept), change the download target, and show scan progress and a list of skipped files
- [ ] T079 [US3] Build the appearance settings in `src/routes/settings/+page.svelte`: theme (system, light, dark), reduce motion (follows the OS by default), and audio format with a note that "MP3 re-encodes (some quality loss)"

**Checkpoint**: US3 works on its own; validate with quickstart V6–V7

---

## Phase 6: User Story 4 - Organize the library (Priority: P4)

**Goal**: tag editing, local playlists, pattern-based renaming with preview and undo, and trash-only deletes

**Independent Test**: [quickstart.md](./quickstart.md) V8

**Depends on**: US3 (library views and TrackList are where users select tracks)

### Tests for User Story 4 ⚠️

- [ ] T080 [P] [US4] Write organizer tests in `src-tauri/tests/organizer.rs`:
  - the pattern tokens `{artist}`, `{album_artist}`, `{album}`, `{track:02}`, `{title}`, `{year}` are rendered and sanitized;
  - `preview_organize` changes nothing on disk;
  - the conflicts `exists`, `invalid_name` and `too_long` are detected;
  - `apply_organize` is refused while conflicts exist unless `skipConflicts`;
  - the undo journal is written before the first move;
  - `undo_organize` restores every path;
  - a partial failure gives the state `partially_applied`
- [ ] T081 [P] [US4] Write tag edit tests in `src-tauri/tests/tag_edit.rs`:
  - `update_tags` writes to the file and to the database together;
  - a scanned file is modified only on an explicit edit;
  - the validation `title` "1–500 chars", `track_number` "≥ 1" and `year` "1000–2100" is applied
- [ ] T082 [P] [US4] Write playlist and delete tests in `src-tauri/tests/playlists.rs`:
  - create, rename, reorder (dense `position` from 0) and delete a local playlist;
  - `export_playlist` writes valid M3U8 with relative paths;
  - `delete_tracks` without `confirm: true` is rejected;
  - with confirmation, files go to the trash and their database rows are removed

### Implementation for User Story 4

- [ ] T083 [US4] Implement the organizer in `src-tauri/src/core/organizer.rs` (pattern rendering, `OrganizePlan` and `OrganizeOp` generation, apply with a JSON undo journal in `<app data>/journals/`, undo) and the commands `preview_organize`, `apply_organize` and `undo_organize` in `src-tauri/src/commands/organize.rs`. T080 must pass
- [ ] T084 [US4] Implement `update_tags` (in `src-tauri/src/commands/organize.rs`, using `core/tagger.rs`) and `delete_tracks` (trash crate, `confirm: true` required) so that T081 and T082 pass
- [ ] T085 [US4] Implement the local playlist commands `create_playlist`, `rename_playlist`, `delete_playlist`, `set_playlist_tracks` and `export_playlist` (m3u8) in `src-tauri/src/commands/playlists.rs`. T082 must pass
- [ ] T086 [P] [US4] Build the tag editor dialog `src/lib/components/TagEditor.svelte`: single and multi-track editing with mixed-value placeholders, an artwork picker through the dialog plugin, and validation messages
- [ ] T087 [US4] Build the Organize screen `src/routes/organize/+page.svelte`:
  - a pattern input with token chips and a live example;
  - a preview table of old → new paths with conflicts highlighted;
  - Apply, Apply skipping conflicts, and Cancel;
  - Undo for the most recent plan
- [ ] T088 [US4] Add the local playlist UI: create, rename and delete in the Playlists tab, drag-to-reorder in `src/routes/library/playlist/[id]/+page.svelte`, "Add to playlist" in the TrackList context menu, and Export M3U8
- [ ] T089 [US4] Add a delete confirmation dialog to `TrackList.svelte` that says the files go to the system trash or recycle bin

**Checkpoint**: US4 works; validate with quickstart V8

---

## Phase 7: User Story 5 - Keep imported playlists in sync (Priority: P5)

**Goal**: a re-sync downloads only new tracks and flags the ones removed online without deleting them

**Independent Test**: [quickstart.md](./quickstart.md) V9

**Depends on**: US1

- [ ] T090 [P] [US5] Write re-sync tests in `src-tauri/tests/resync.rs`, using the fixture before/after versions of a playlist:
  - only the added tracks create jobs;
  - removed tracks get `removed_online_at` set, and their files and Track rows are kept;
  - re-adding a removed track clears `removed_online_at`;
  - `last_synced_at` is updated
- [ ] T091 [US5] Implement `resync_playlist` in `src-tauri/src/core/importer.rs` (diff by `SourceRef` against the current `PlaylistEntry` rows) and `src-tauri/src/commands/import.rs`. It returns `{ batchId, added, removedOnline }`. T090 must pass
- [ ] T092 [US5] Add a Re-sync button and a "last synced" time to imported playlist views (`src/routes/library/playlist/[id]/+page.svelte`), with a "removed online" badge and filter, and a "Remove local files too" action that goes through the US4 delete confirmation when US4 is present

**Checkpoint**: all user stories work

---

## Phase 8: Polish & Cross-Cutting Concerns

- [ ] T093 [P] Run the startup performance pass: measure the cold start to the library view (target < 3 s, SC-007), defer the scan and tool update checks until after the first paint, and add the timing to the debug log
- [ ] T094 [P] Run the Linux WebKitGTK smoothness pass: profile the library scroll and transitions on Ubuntu 22.04, replace any `filter` or `backdrop-filter` effects that cost too much, and confirm that reduce motion turns off non-essential animation
- [ ] T095 [P] Run the accessibility pass across `src/lib/components/` and the routes: focus rings, ARIA labels on icon buttons, contrast ≥ 4.5:1 in both themes, and full keyboard navigation
- [ ] T096 [P] Set up the app icons and packaging in `src-tauri/icons/` and `src-tauri/tauri.conf.json`: macOS DMG, Windows NSIS + MSI, Linux AppImage + deb + rpm. Add a release workflow `.github/workflows/release.yml` that builds on all three OSes
- [ ] T097 [P] Configure tauri-plugin-updater (signed updates, endpoint in `tauri.conf.json`), with an "Update available" toast in `src/routes/+layout.svelte`
- [ ] T098 [P] Write the user documentation in `README.md`: what the app does, the personal-use and legal note, install steps per OS, the account setup guides (Spotify Premium and own app, Google Cloud OAuth client, SoundCloud Artist Pro), and the Linux Secret Service note
- [ ] T099 Run the security pass: check that `grep -ri "token\|secret" <app log dir>` finds no values after a sign-in and import; review `src-tauri/capabilities/` for least privilege (shell is limited to the managed tool paths, fs scope to watched folders); and check the CSP in `tauri.conf.json`
- [ ] T100 Run [quickstart.md](./quickstart.md) V1–V9 on Windows, Linux and macOS, record the results in `specs/001-music-library-organizer/checklists/validation.md`, and fix any differences between the systems (SC-008)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)** has no dependencies.
- **Foundational (Phase 2)** depends on Setup and **blocks all stories**.
- **US1 (P1)** depends only on Foundational.
- **US2 (P2)** depends on US1: matches and the queue come from the importer.
- **US3 (P3)** depends only on Foundational. It can run in parallel with US1, because scanning creates
  library content without importing.
- **US4 (P4)** depends on US3, which provides the TrackList and library views.
- **US5 (P5)** depends on US1, which provides the importer and imported playlists.
- **Polish** comes after the stories you plan to ship.

```text
Setup → Foundational ─┬─► US1 ─┬─► US2
                      │        └─► US5
                      └─► US3 ───► US4
                                         ──► Polish
```

### Within each story

The tests are written first and must fail. Then core logic, then commands, then the UI. Each task
says which test it must make pass.

### Key task dependencies

- T019 → T020 → every database-backed task
- T022 → T045, T046 and all adapters
- T023 + T024 → T034–T036 and T047–T049
- T051 + T052 + T053 → T054 → T055 → T058 and T059
- T072 + T073 → T075–T078
- T075 → T088 and T089

---

## Parallel Examples

### Phase 2

```text
T010 sanitize tests │ T011 dedup tests │ T012 net tests │ T013 db tests │ T014 secret tests
then: T016 │ T017 │ T018 │ T021 │ T022 │ T025 │ T026 │ T029 │ T030 │ T032
```

### User Story 1

```text
Tests:     T034 youtube │ T035 spotify │ T036 soundcloud │ T037 matcher │ T038 accuracy
           T039 importer │ T040 queue │ T041 tagger │ T042 finalize │ T043 oauth │ T044 e2e
Adapters:  T047 youtube.rs │ T048 spotify.rs │ T049 soundcloud.rs   (after T045/T046)
UI:        T057 queue store │ (then T058, T059, T060, T061)
```

### US1 ∥ US3 (two developers after Foundational)

```text
Dev A: T034 → … → T061 (US1)
Dev B: T069 → … → T079 (US3)
```

---

## Implementation Strategy

### MVP first (US1)

1. Phase 1 Setup, then Phase 2 Foundational.
2. Phase 3 (US1). If you want an even smaller first slice, do public links first (T045–T055,
   T057–T059, T061) and sign-in afterwards (T043, T056, T060).
3. **Stop and validate** with quickstart V1–V5 on all three systems → MVP release.

### Incremental delivery

MVP (US1) → + US2 (match review) → + US3 (library and scan) → + US4 (organize) → + US5 (re-sync)
→ Polish. Each step is a working app that can be shipped.

---

## Notes

- `[P]` = different files with no unfinished dependencies.
- Commit after each task or logical group, with a message that explains why (constitution Workflow).
- Never call live platform APIs in tests. Live checks only run with `cargo test --features live -- --ignored`.
- Regenerate `src/lib/bindings.ts` (`pnpm bindings`) whenever a command or event changes, and commit it.
