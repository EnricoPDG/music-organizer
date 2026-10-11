# Data Model: Music Library Organizer

**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

Everything is stored in a single SQLite database at `<app data dir>/library.db`; see
[research.md R8](./research.md#r8-storage--search). IDs are random UUIDv7 values stored as TEXT.
Timestamps are ISO-8601 UTC. Paths are stored as absolute, OS-native strings.

## Entity overview

```text
Platform (code-registered, not a table)
   │
   ├── SourceRef ──┐            ConnectedAccount
   │               ▼
   │            Track ◄── Match ── MatchCandidate
   │            ▲   ▲
   │            │   └── PlaylistEntry ── Playlist (kind: imported | local)
   │            │
   │       DownloadJob ── ImportBatch
   │
WatchedFolder ── (Track.file_path inside it)
Settings (key/value)        OrganizePlan ── OrganizeOp
```

## Track

A song in the library. A track is either downloaded by the app or scanned from a watched folder.

| Field | Type | Rules |
|-------|------|-------|
| id | uuid | PK |
| title | text | required, 1–500 chars |
| artists | json array of text | at least 1 entry; display string = joined with ", " |
| album | text? | |
| album_artist | text? | defaults to the first artist when tagging |
| track_number | int? | ≥ 1 |
| year | int? | 1000–2100 |
| duration_ms | int? | > 0 |
| isrc | text? | 12 chars `^[A-Z]{2}[A-Z0-9]{3}\d{7}$` |
| artwork_hash | text? | SHA-256 of the image in the artwork cache |
| file_path | text? | unique when set; null until downloaded |
| file_format | enum? | `m4a`, `opus`, `mp3`, `flac`, `ogg`, `wav` |
| file_mtime / file_size | int? | used to detect external changes (FR-019) |
| origin | enum | `downloaded`, `scanned` |
| status | enum | `pending`, `available`, `missing` (file gone), `failed` |
| dedup_key | text | `norm(primary artist) + "\|" + norm(title)` + duration bucket (±3 s), indexed |
| created_at / updated_at | timestamp | |

**Validation and behavior**

- `dedup_key` is used for FR-014 and FR-015b. Before creating a track, the app looks up the key and
  links to the existing track instead. `norm()` lower-cases the text, strips diacritics and
  punctuation, and removes "feat." clauses and "(Official Video)"-style suffixes.
- Tags of `origin = scanned` tracks are only written when the user edits the track
  (constitution Principle III, FR-015).
- An FTS5 virtual table `track_fts(title, artists, album)` is kept in sync by triggers (FR-017).

**Track status transitions**

```text
pending ──download ok──► available ──file deleted externally──► missing
   │                         ▲                                     │
   └──download failed──► failed    └────file reappears / relinked───┘
```

## Artist / Album (derived)

Artists and albums are **SQL views** over `Track`, not stored tables (Principle VI):

- an artist comes from the distinct values in `artists` and `album_artist`;
- an album groups tracks by `(album_artist, album)`, and its artwork is the most common
  `artwork_hash` among them.

## SourceRef

Where a track came from. A track can have several (e.g. the same song in Spotify and in SoundCloud).

| Field | Type | Rules |
|-------|------|-------|
| id | uuid | PK |
| track_id | uuid | FK → Track, cascade |
| platform | text | registered platform id: `spotify`, `youtube`, `soundcloud`, … |
| platform_track_id | text | required |
| url | text | |
| raw_metadata | json | the `NormalizedTrack` as received (see the adapter contract) |

Unique on `(platform, platform_track_id)`. This makes re-imports idempotent (FR-014).

## Match

The chosen YouTube audio source for a track. There is exactly one Match per track. For tracks that
come from YouTube itself, `method = direct` and `confidence = 1`.

| Field | Type | Rules |
|-------|------|-------|
| track_id | uuid | PK/FK → Track |
| youtube_video_id | text | 11 chars |
| confidence | real | 0–1 |
| confidence_level | enum | `high` ≥ 0.85, `medium` ≥ 0.65, `low` |
| method | enum | `direct`, `auto`, `user_candidate`, `user_url` |
| signals | json | the score breakdown that explains the choice (Principle IV) |
| decided_at | timestamp | |

User decisions (`user_*`) are never overwritten by automatic re-matching (FR-007).

## MatchCandidate

| Field | Type | Rules |
|-------|------|-------|
| track_id | uuid | FK → Match |
| youtube_video_id | text | |
| title, channel | text | |
| duration_ms | int? | |
| score | real | |
| rank | int | 1 = best; at least 3 are kept when available (spec US2) |

## Playlist

Holds both imported and local playlists, told apart by `kind`.

| Field | Type | Rules |
|-------|------|-------|
| id | uuid | PK |
| kind | enum | `imported`, `local` |
| name | text | required |
| platform / platform_playlist_id / url | text? | required when `kind = imported`; unique together |
| owner_name | text? | |
| artwork_hash | text? | |
| last_synced_at | timestamp? | |
| fetch_mode | enum? | `official`, `public_fallback` (Spotify FR-004b; shown in the UI) |

## PlaylistEntry

| Field | Type | Rules |
|-------|------|-------|
| playlist_id | uuid | FK, cascade |
| track_id | uuid | FK |
| position | int | dense order, starting at 0 |
| removed_online_at | timestamp? | set when a re-sync finds the track gone (FR-024); the file is kept |

The same track in many playlists is stored once, with one row here per playlist (edge case).

## ImportBatch & DownloadJob

| ImportBatch field | Type | Rules |
|-------|------|-------|
| id | uuid | PK |
| playlist_id | uuid? | |
| source_url | text | |
| created_at / finished_at | timestamp | |

| DownloadJob field | Type | Rules |
|-------|------|-------|
| id | uuid | PK |
| batch_id | uuid | FK |
| track_id | uuid | FK; unique per active job, so a track is never downloaded twice at once |
| state | enum | see below |
| progress | real | 0–1 |
| attempts | int | ≤ 5 automatic retries |
| error_kind | enum? | `not_found`, `unavailable`, `age_restricted`, `region_blocked`, `rate_limited`, `network`, `disk_full`, `tool_missing`, `unknown` |
| error_message | text? | readable reason (FR-012) |
| temp_path | text? | the `.part` file, deleted on cancel or failure |
| updated_at | timestamp | |

**DownloadJob state machine** (Principle IV: every job ends in a visible final state)

```text
queued → matching → awaiting_review* → downloading → tagging → finalizing → done
   │         │                             │             │            │
   └─────────┴───────────── paused ◄───────┴─────────────┘            │
                     (user pause / network lost)                      │
any non-final ──► failed (after retries)    any non-final ──► cancelled
queued ──► skipped (already in library — dedup)
```

\* `awaiting_review` happens only when the user has turned on "review low-confidence matches before
downloading" and the match is `low`.

Final states are `done`, `skipped`, `failed` and `cancelled`. On app start, jobs left in
`downloading`, `tagging` or `finalizing` return to `queued`, and their `.part` files are deleted
(spec US1 scenario 7).

## ConnectedAccount

| Field | Type | Rules |
|-------|------|-------|
| platform | text | PK |
| display_name | text | |
| client_id | text | the user's own developer app (research R6); not secret |
| keyring_key | text | reference to the OS credential store entry holding the tokens and client secret |
| scopes | json | |
| status | enum | `connected`, `expired`, `error` |
| connected_at | timestamp | |

Disconnecting deletes the credential store entry and this row (FR-004a). Tokens never touch the
database.

## WatchedFolder

| Field | Type | Rules |
|-------|------|-------|
| id | uuid | PK |
| path | text | unique; must exist and be readable when added |
| is_download_target | bool | exactly one folder is the download target (the "music folder") |
| last_scan_at | timestamp? | |

Removing a folder deletes its `scanned` tracks from the database only. Files are never touched
(FR-015a).

## OrganizePlan & OrganizeOp

A rename/move/retag preview that the user can apply or cancel (FR-022, Principle III).

| OrganizePlan | OrganizeOp |
|--------------|------------|
| id, pattern, created_at, state (`previewed`, `applied`, `cancelled`, `partially_applied`), journal_path | plan_id, track_id, op (`move`, `retag`), from_path, to_path, conflict (`none`, `exists`, `invalid_name`, `too_long`) |

A plan cannot be applied while any operation has a conflict other than `none`, unless the user
chooses "skip conflicting". Applying writes a JSON undo journal before moving any file.

## Settings (key/value)

| Key | Default |
|-----|---------|
| `audio_format` | `m4a` |
| `naming_pattern` | `{album_artist}/{album}/{track:02} - {title}` |
| `theme` | `system` (`light`, `dark`) |
| `reduce_motion` | follows the OS setting |
| `max_parallel_downloads` | `3` (1–5) |
| `review_low_confidence` | `true` |
| `tool_paths` | auto-managed (optional user override, research R4) |
