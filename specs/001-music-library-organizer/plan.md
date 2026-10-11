# Implementation Plan: Music Library Organizer

**Branch**: `001-music-library-organizer` | **Date**: 2026-10-11 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/001-music-library-organizer/spec.md`

## Summary

This is a cross-platform desktop app (Windows, Linux, macOS) that:
- imports playlists from Spotify, YouTube and SoundCloud through pluggable platform adapters;
- matches non-YouTube tracks to YouTube sources with an explainable confidence score;
- downloads tagged audio safely, using temp files and atomic moves;
- lets the user browse, search and organize a local library that also includes existing music
  folders.

The user asked for "the best tech stack for a desktop app for multiple OS, beautiful and smooth".
The chosen stack:
- **Tauri 2** with a **Rust** backend for scanning, tagging, the download queue, OAuth and SQLite;
- **Svelte 5 + Tailwind 4 + Bits UI** for the interface;
- **yt-dlp + Deno + FFmpeg** as tools the app downloads and updates itself.

Rationale and alternatives are in [research.md](./research.md) (R1–R4).

## Technical Context

**Language/Version**: Rust stable ≥ 1.85 (edition 2024) for the backend; TypeScript 5.x + Svelte 5 for
the UI

**Primary Dependencies**:
- **Backend (Rust)**: Tauri 2.12 (plugins: dialog, opener, updater), tauri-specta 2.0.0-rc.25
  (pinned), tokio 1.53, reqwest 0.13, rusqlite 0.40 (bundled, FTS5), lofty 0.25, keyring 4.2,
  trash 5.2, notify 8.2, walkdir 2.5, governor 0.10, strsim 0.11, zip 9.
  OAuth is implemented on reqwest (research R3a).
- **Frontend**: SvelteKit 3 (adapter-static, SPA mode), Svelte 5.57, Tailwind CSS 4.3, Bits UI 2.19,
  @tanstack/virtual-core 3.19, Vite 8.
- **External tools**: yt-dlp ≥ 2026.08.19 and Deno 2.9, which the app installs and updates itself.
  FFmpeg is optional and only needed for Opus/MP3 (research R4 implementation notes).

**Storage**: SQLite (WAL + FTS5) in the app data folder, artwork cache on disk, and audio files in
user folders. Tokens are kept in the OS credential store.

**Testing**:
- Rust: `cargo test` with recorded fixtures, adapter contract suites and a matcher accuracy set.
- Frontend: Vitest 5 for logic and Playwright 1.64 against mocked Tauri IPC.
- Live checks: opt-in only.
- CI: GitHub Actions matrix on all three operating systems.

**Target Platform**: Windows 10/11 (WebView2), macOS 12+ (WKWebView), and Linux x86_64 with
WebKitGTK 4.1 (Ubuntu 22.04+, Fedora 38+), shipped as AppImage, deb and rpm.

**Project Type**: desktop-app (a Tauri app: a Rust core with a web UI in one repository)

**Performance Goals**:
- 60 fps scrolling for 10k+ track lists;
- search in under 100 ms in the backend and under 0.5 s end to end (SC-006);
- cold start under 3 s (SC-007);
- scanning 5,000 files in under 2 minutes (SC-010);
- progress events batched to at most 10 per second.

**Constraints**:
- browsing and organizing work offline;
- no partial files in the library;
- no secrets in logs or the database;
- per-platform rate limits;
- installer under about 20 MB, because the tools are downloaded separately;
- the same features on all three operating systems.

**Scale/Scope**: a single user; libraries up to about 50k tracks; playlists up to 5k tracks; 6 main
screens; 3 platform adapters at launch.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| # | Principle / Rule | Pre-research | Post-design | Evidence |
|---|------------------|:---:|:---:|----------|
| I | Pluggable Platform Adapters: shared contract, platform code only in adapters, normalized track model | ✅ | ✅ | [contracts/platform-adapter.md](./contracts/platform-adapter.md). The core uses only `PlatformAdapter` and `NormalizedTrack`. Adding a platform = a module + one registry line + the contract suite |
| II | Local-First Desktop App: no backend server, inspectable local data, credentials only in the OS store, works offline | ✅ | ✅ | SQLite and files ([data-model.md](./data-model.md)); keyring (R11); `Secret<T>` redaction; "bring your own app" OAuth (R6); offline browse and organize (FR-029) |
| III | The User's Library Is Never Damaged: confirmation, atomic downloads, idempotency, previews | ✅ | ✅ | `.part` files with atomic rename; `dedup_key` + unique `SourceRef`; OrganizePlan preview + undo journal; trash, not delete; scanned files read-only (R10) |
| IV | Resilient, Rate-Aware Networking: each track independent, visible final state, backoff, reviewable matches | ✅ | ✅ | DownloadJob state machine with final states; governor + backoff + `Retry-After` (R12); `Match.signals` and the candidates UI (R7) |
| V | Test-First Core, No Live Network in Tests | ✅ | ✅ | Fixtures and a fake `ToolRunner`; `adapter_contract_suite!`; live tests behind `--features live --ignored` (R13) |
| VI | Simplicity | ✅ | ✅ | One Tauri project; Artist and Album are SQL views; synchronous rusqlite. New dependencies are justified in research.md. Deviations are tracked below |
| C1 | Windows, Linux and macOS first-class; OS-specific code isolated; OS-aware paths | ✅ | ✅ | `src-tauri/src/os/` module; one file-name sanitizer; CI matrix; quickstart run on all three |
| C2 | No DRM circumvention or auth bypass; personal use | ✅ | ✅ | Only public data or the user's own OAuth access. The Spotify public fallback reads a publicly served page with no login (see Complexity Tracking) |
| C3 | Files tagged with normalized metadata | ✅ | ✅ | lofty tagging step in the job pipeline (R5) |
| DoD | Tests pass offline on all three operating systems; contract tests for each adapter; acceptance checked in the real app | ✅ | ✅ | [quickstart.md](./quickstart.md) V1–V9 and its Done-when section |

**Gate result**: PASS. No unjustified violations. The three accepted risks are in Complexity Tracking.

## Project Structure

### Documentation (this feature)

```text
specs/001-music-library-organizer/
├── plan.md              # This file
├── research.md          # Phase 0: decisions R1–R13
├── data-model.md        # Phase 1: entities, state machines
├── quickstart.md        # Phase 1: run + validation scenarios V1–V9
├── contracts/
│   ├── platform-adapter.md
│   └── ipc.md
├── checklists/requirements.md
└── tasks.md             # Phase 2 (/speckit-tasks — not created here)
```

### Source Code (repository root)

```text
package.json  pnpm-lock.yaml  svelte.config.js  vite.config.ts  tsconfig.json
src/                                   # Svelte 5 UI (SvelteKit, SPA)
├── app.css                            # Tailwind 4 + theme tokens (light/dark)
├── lib/
│   ├── bindings.ts                    # GENERATED by tauri-specta — do not edit
│   ├── api/                           # thin wrappers + event subscriptions
│   ├── stores/                        # runes-based state: queue, library, settings
│   ├── components/                    # design system (Bits UI based), TrackRow, ArtworkImg, Badge…
│   └── utils/
├── routes/
│   ├── +layout.svelte                 # shell: sidebar, ⌘/Ctrl+K search, theme
│   ├── import/  queue/  review/  library/  organize/  settings/
└── tests/                             # Vitest unit tests
e2e/                                   # Playwright specs against mocked IPC

src-tauri/
├── Cargo.toml  tauri.conf.json  capabilities/
├── src/
│   ├── main.rs  lib.rs                # app setup, command registration, specta export
│   ├── commands/                      # IPC layer only: import, jobs, matching, library, organize, accounts, settings
│   ├── core/
│   │   ├── model.rs                   # NormalizedTrack, Track, ids
│   │   ├── importer.rs                # collection → tracks (dedup) → matches → jobs
│   │   ├── matcher.rs                 # scoring + confidence (R7)
│   │   ├── queue.rs                   # DownloadJob state machine, concurrency, resume
│   │   ├── tagger.rs                  # lofty read/write
│   │   ├── organizer.rs               # pattern → OrganizePlan, apply, undo journal
│   │   ├── scanner.rs                 # walkdir + notify
│   │   └── sanitize.rs                # cross-OS filename/path rules
│   ├── platforms/
│   │   ├── mod.rs                     # PlatformAdapter trait, registry()
│   │   ├── youtube.rs  spotify.rs  soundcloud.rs
│   ├── tools/                         # ToolRunner trait, yt-dlp/deno/ffmpeg manager + updater
│   ├── net/                           # http client, rate limiters, retry, OAuth loopback
│   ├── db/                            # rusqlite connection thread, migrations/, queries
│   ├── secrets.rs                     # keyring + Secret<T>
│   └── os/                            # per-OS: credential store quirks, reveal, paths
└── tests/
    ├── fixtures/{spotify,youtube,soundcloud,ytdlp}/
    ├── contract_youtube.rs  contract_spotify.rs  contract_soundcloud.rs
    ├── matcher_accuracy.rs  queue.rs  organizer.rs  scanner.rs  sanitize.rs
.github/workflows/ci.yml               # matrix: ubuntu, windows, macos
```

**Structure Decision**: a single Tauri project, with `src/` for the UI and `src-tauri/` for the
Rust core. This is the standard Tauri layout. Separate workspace crates were rejected (Principle VI).
The core is split into `core/`, `platforms/`, `tools/`, `net/` and `db/` modules, so that the
`commands/` layer stays thin and the core logic can be tested with no Tauri runtime.

## Complexity Tracking

| Deviation / accepted risk | Why needed | Simpler alternative rejected because |
|---------------------------|------------|--------------------------------------|
| Spotify public-playlist fallback reads Spotify's public embed page (unofficial) | Since February 2026, the official API returns the track list only for playlists the user owns. The user chose to keep "paste any public link" working (FR-004b) | Official API only: drops a use case the user explicitly wanted. The fallback is isolated in `spotify.rs`, reported as `fetchMode = public_fallback`, and fails with a clear `PublicSourceChanged` error |
| External tools (yt-dlp, Deno, FFmpeg) downloaded at runtime instead of bundled | YouTube breaks extractors often, so yt-dlp needs updates faster than app releases. Bundling would add about 150 MB to the installer | Bundled sidecars freeze the yt-dlp version. A pure-Rust extractor lags behind YouTube changes (R4). Downloads are verified by checksum, and a user path can override them |
| tauri-specta pinned at a release candidate (2.0.0-rc.25) | Generated, type-checked IPC bindings stop the Rust and TS contracts from drifting apart (R3) | Hand-written types drift silently. Risk is limited by the exact pin and the CI binding check, with hand-written bindings as fallback |
