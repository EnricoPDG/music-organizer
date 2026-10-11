# Contract: UI ↔ Backend IPC

The frontend talks to the Rust backend only through the Tauri commands and events defined here.
TypeScript bindings are **generated** from the Rust definitions with tauri-specta into
`src/lib/bindings.ts` (research R3). CI fails if the generated file is out of date.

## Conventions

- Commands are `snake_case` in Rust and `camelCase` in TypeScript. Every command returns
  `Result<T, AppError>`.
- `AppError = { kind: ErrorKind, message: string, retryable: boolean }`. `message` is safe to show
  to the user and never contains secrets.
- Long-running operations return an id immediately and report progress through events. Commands
  never block the UI.
- List commands are paged: `{ offset, limit ≤ 500 }` → `{ items, total }`.

## Commands

### Import (US1, US5)

| Command | Input | Output | Notes |
|---------|-------|--------|-------|
| `resolve_link` | `{ url }` | `LinkPreview { platform, kind, name, owner, artworkUrl, trackCount, fetchMode, requiresSignIn }` | Fast preview. Errors: `invalid_url`, `unsupported_platform`, `private`, `public_source_changed` |
| `start_import` | `{ url, autoDownload: bool }` | `{ batchId, playlistId }` | Fetches, deduplicates, matches, and queues downloads |
| `resync_playlist` | `{ playlistId }` | `{ batchId, added, removedOnline }` | Downloads only new tracks (FR-024) |
| `list_my_collections` | `{ platform }` | `CollectionSummary[]` | Requires a connected account |

### Downloads (US1)

| Command | Input | Output |
|---------|-------|--------|
| `list_jobs` | `{ batchId? , states? }` | `DownloadJob[]` |
| `pause_jobs` / `resume_jobs` / `cancel_jobs` / `retry_jobs` | `{ jobIds? , batchId? }` | `{ affected }` |

### Matching (US2)

| Command | Input | Output |
|---------|-------|--------|
| `get_match` | `{ trackId }` | `MatchDetail { chosen, confidenceLevel, signals, candidates[≥3 if available] }` |
| `set_match` | `{ trackId, candidateVideoId? , youtubeUrl? }` | `MatchDetail`. Sets `method = user_*` and queues a re-download if the track was already downloaded |
| `search_candidates` | `{ trackId, query? }` | `MatchCandidate[]` |

### Library (US3)

| Command | Input | Output |
|---------|-------|--------|
| `list_tracks` | `{ offset, limit, sort, filter? { artist?, album?, playlistId? } }` | `Page<TrackRow>` |
| `list_artists` / `list_albums` | paging | `Page<ArtistRow>` / `Page<AlbumRow>` |
| `search` | `{ query, limit ≤ 50 }` | `{ tracks, artists, albums }`. Must answer within 100 ms for 10k tracks (SC-006 budget) |
| `get_artwork` | `{ hash, size: 64 \| 256 \| 600 }` | asset URL (served through Tauri's asset protocol, cached) |
| `reveal_in_folder` / `open_with_default` | `{ trackId }` | `()` |
| `add_watched_folder` / `remove_watched_folder` / `rescan` | `{ path }` / `{ folderId }` / `{}` | `{ scanId }` |

### Organize (US4)

| Command | Input | Output |
|---------|-------|--------|
| `update_tags` | `{ trackIds, patch { title?, artists?, album?, trackNumber?, year?, artworkPath? } }` | `TrackRow[]`. Writes to the file and the database together |
| `preview_organize` | `{ trackIds, pattern }` | `OrganizePlan { id, ops[ { from, to, conflict } ] }`. Changes nothing on disk |
| `apply_organize` | `{ planId, skipConflicts: bool }` | `{ applied, skipped }` |
| `undo_organize` | `{ planId }` | `{ restored }` |
| `delete_tracks` | `{ trackIds, confirm: true }` | `{ trashed }`. Moves files to the system trash (FR-023) |
| `create_playlist` / `rename_playlist` / `delete_playlist` | … | `Playlist` |
| `set_playlist_tracks` | `{ playlistId, trackIds (ordered) }` | `()` |
| `export_playlist` | `{ playlistId, path, format: "m3u8" }` | `()` |

### Accounts & settings

| Command | Input | Output |
|---------|-------|--------|
| `list_accounts` | `{}` | `AccountStatus[]` for every adapter with `sign_in` |
| `connect_account` | `{ platform, clientId, clientSecret? }` | `AccountStatus`. Opens the system browser and waits for the loopback callback (timeout 5 min) |
| `disconnect_account` | `{ platform }` | `()`. Removes the credential store entry (FR-004a) |
| `get_settings` / `update_settings` | `{}` / `Partial<Settings>` | `Settings` |
| `tools_status` / `install_tools` | `{}` | `ToolsStatus { ytDlp, deno, ffmpeg: { installed, version, updateAvailable } }` |

## Events (backend → UI)

| Event | Payload | Rate |
|-------|---------|------|
| `job://progress` | `{ jobId, trackId, state, progress, errorKind?, errorMessage? }[]` | **Batched**, at most 10 updates per second in total, so the UI stays smooth (FR-027) |
| `batch://summary` | `{ batchId, total, done, failed, skipped, active }` | Every time a job reaches a final state |
| `scan://progress` | `{ scanId, found, processed, skipped[] }` | At most 4 per second |
| `library://changed` | `{ trackIds?, scope: "tracks" \| "playlists" \| "all" }` | After each write, so views refresh |
| `account://status` | `AccountStatus` | When a sign-in expires or is reconnected |
| `tools://status` | `ToolsStatus` | During tool install and update |

## UI contract (screens)

| Screen | Purpose | Key states |
|--------|---------|------------|
| Import (home) | Paste a link; show a preview card; list "My playlists" from connected accounts | empty, resolving, preview, error (with a sign-in suggestion when `private`) |
| Queue | Live batch and job list, with pause/resume/cancel/retry | grouped by batch; failed jobs show their reason and a "Fix match" action |
| Match review | Chosen source and candidates, with confidence badges | low-confidence first |
| Library | Tracks / Artists / Albums / Playlists tabs, plus global search (⌘/Ctrl+K) | virtualized lists, artwork grid |
| Organize | Tag editor, naming pattern with a preview table, conflicts highlighted | preview → apply → undo |
| Settings | Folders, format, theme, motion, accounts (guided setup), tools | |

All screens support light and dark themes, keyboard navigation, and "reduce motion" (FR-026).
