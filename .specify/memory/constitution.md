# Music Organizer v2 Constitution

## Core Principles

### I. Pluggable Platform Adapters

The app imports playlists from N platforms (initially Spotify, YouTube and SoundCloud) and
downloads audio via YouTube.

- Every platform MUST be integrated through a single shared adapter contract (e.g. resolve URL,
  list playlist tracks, fetch track metadata).
- Platform-specific code MUST live only inside its adapter. The core (library, matching, download
  queue, UI) MUST NOT reference a specific platform by name.
- Adding a new platform MUST NOT require changes to the core beyond registering the adapter.
- Tracks from every platform MUST be normalized into one internal track model (title, artists,
  album, duration, source URL, platform ID) before they reach the core.

Rationale: "N platforms" is a core requirement. A shared contract keeps each new platform cheap
to add and keeps platform API churn contained.

### II. Local-First Desktop App

- The app MUST run entirely on the user's machine as a desktop application. No project-owned
  backend server is required for any feature.
- The music library, its metadata and the app state MUST be stored locally in formats the user
  can inspect (e.g. plain audio files plus an open database or JSON).
- Platform credentials or tokens MUST be stored only locally, preferably in the OS credential
  store (macOS Keychain, Windows Credential Manager or Linux Secret Service). They MUST never be
  logged or sent anywhere except to the platform that issued them.
- Features that need the network MUST degrade gracefully when offline. Browsing and organizing
  the existing library MUST keep working.

Rationale: the user owns their music and data. A local-first design avoids hosting costs and
privacy risks.

### III. The User's Library Is Never Damaged

- The app MUST NOT delete, overwrite or move existing user files without explicit user
  confirmation.
- Downloads MUST be written to a temporary file and moved into place only after they complete,
  so an interrupted download never leaves a corrupt track in the library.
- Imports and downloads MUST be idempotent. Re-importing the same playlist MUST NOT create
  duplicate files or duplicate library entries.
- Bulk reorganization operations (rename, retag, move) MUST offer a preview before they run.

Rationale: a music organizer that loses or corrupts files breaks user trust and cannot be fixed
afterwards.

### IV. Resilient, Rate-Aware Networking

- Playlist imports and batch downloads MUST process tracks individually. If one track fails, the
  rest of the batch MUST still complete.
- Each track MUST end in a visible status (e.g. queued, downloading, done, skipped, failed with
  a reason).
- Network calls MUST use timeouts, bounded retries with backoff, and pacing between requests so
  they respect platform rate limits.
- When a track from a non-YouTube platform is matched to a YouTube video, the match MUST be
  recorded together with how it was chosen, and the user MUST be able to review and correct it.

Rationale: third-party platforms are unreliable and throttle aggressive clients. Large
playlists must still finish, and wrong matches must be easy to see and fix.

### V. Test-First Core, No Live Network in Tests

- Core logic (track normalization, matching, deduplication, library operations, queue
  behavior) MUST be developed test-first: write the test, see it fail, then implement.
- Automated tests MUST NOT call live platform APIs. Adapters MUST be tested against recorded or
  hand-written fixtures.
- Every adapter MUST have contract tests showing that it satisfies the shared adapter contract
  (Principle I).
- Optional manual or smoke checks against live platforms MAY exist, but MUST be kept separate
  from the default test run.

Rationale: live APIs are slow, flaky and rate-limited. Fixtures keep the test suite fast and
deterministic.

### VI. Simplicity

- Build the smallest thing that meets the current spec (YAGNI). Speculative abstractions,
  plugins or settings MUST NOT be added before a spec needs them.
- New dependencies MUST be justified in the plan. Prefer the standard library and well-maintained,
  widely used packages.
- Any deviation from these principles MUST be recorded in the plan's Complexity Tracking section
  with a justification.

Rationale: this is a personal project. Keeping it simple keeps it maintainable.

## Platform & Technical Constraints

- **Form factor**: A native or packaged desktop application.
- **Supported operating systems**: Windows, Linux and macOS are all first-class targets. Every
  feature MUST work on all three. Platform-specific code (paths, keychain, file pickers,
  notifications) MUST be isolated behind an OS abstraction, and file paths MUST be built with
  OS-aware path APIs, never hard-coded separators.
- **Packaging**: Each release MUST produce an installable build for Windows, Linux and macOS.
- **Tech stack**: Not fixed by this constitution. The language and framework are chosen and
  justified in `/speckit-plan` for the first feature. TODO(TECH_STACK).
- **Audio source**: Audio is downloaded from YouTube. Spotify and SoundCloud provide playlist and
  track metadata that is matched to YouTube sources (Principle IV).
- **Legal & ethical use**: The app is intended for personal use. It MUST NOT circumvent DRM or
  bypass platform authentication. Users are responsible for complying with each platform's terms.
- **Output**: Downloaded files MUST be tagged with the normalized metadata (title, artist, album
  and artwork when available).

## Development Workflow & Quality Gates

- Features follow the Spec Kit flow: `/speckit-specify` → (`/speckit-clarify`) → `/speckit-plan` →
  `/speckit-tasks` → (`/speckit-analyze`) → `/speckit-implement`.
- Every plan MUST include a Constitution Check that confirms compliance with Principles I–VI
  before design and again after it.
- A feature is done only when:
  - all tests pass without network access on Windows, Linux and macOS (e.g. via a CI matrix);
  - every new platform integration has passing adapter contract tests;
  - the user-facing behavior described in the spec's acceptance scenarios has been checked in the
    running desktop app.
- Each change MUST be small and self-contained, with a commit message that states why it was made.

## Governance

- This constitution overrides all other project practices. If a spec, plan or task conflicts with
  it, the constitution wins until it is amended.
- **Amendments**: Change this file through `/speckit-constitution`, describe the change in the Sync
  Impact Report, and update any affected specs or plans in the same change.
- **Versioning**: Semantic versioning.
  - MAJOR: a principle is removed or redefined in a backward-incompatible way.
  - MINOR: a principle or section is added or materially expanded.
  - PATCH: clarifications and wording changes.
- **Compliance review**: `/speckit-plan` (Constitution Check) and `/speckit-analyze` MUST check
  artifacts against this document. Unjustified violations block implementation.

**Version**: 1.0.0 | **Ratified**: 2026-10-11 | **Last Amended**: 2026-10-11
