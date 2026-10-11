# Feature Specification: Music Library Organizer

**Feature Branch**: `001-music-library-organizer`

**Created**: 2026-10-11

**Status**: Draft

**Input**: User description: "Desktop app to organize my music library: import playlists from Spotify,
YouTube and SoundCloud (extensible to more platforms), match tracks to YouTube, download the audio with
tags, and browse/organize the local library. Use the best tech stack for desktop app to make more
beatifull and smoth"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Import a playlist and download it (Priority: P1)

The user pastes a playlist link (or a single track link) from Spotify, YouTube or SoundCloud. The app
shows the playlist name and its tracks, finds a matching YouTube audio source for each track, and
downloads each one into the user's music folder as a tagged audio file (title, artist, album,
artwork). The user watches each track's progress and final status.

**Why this priority**: This is the core reason the app exists. On its own it already turns an online
playlist into a local, tagged music collection.

**Independent Test**: Paste a public playlist link from each of the three platforms, start the import,
and confirm that the tracks appear as correctly tagged audio files in the chosen folder.

**Acceptance Scenarios**:

1. **Given** a valid public Spotify playlist link, **When** the user imports it, **Then** the app lists
   every track with its title, artists and duration before downloading starts.
2. **Given** an imported playlist, **When** the user starts the download, **Then** each track shows a
   live status (queued, matching, downloading, done, skipped, failed) and overall progress.
3. **Given** a completed download, **When** the user opens the file in any common music player,
   **Then** the title, artist, album and artwork tags are present and correct.
4. **Given** a playlist where one track cannot be found or downloaded, **When** the import runs,
   **Then** that track is marked failed with a readable reason and all other tracks still complete.
5. **Given** the user has connected their Spotify account, **When** they open the import screen,
   **Then** their private playlists and Liked songs are listed and can be imported the same way.
6. **Given** a playlist that was already imported, **When** the user imports it again, **Then** no
   duplicate files or library entries are created.
7. **Given** a download in progress, **When** the user closes the app, **Then** no partial or corrupt
   file is left in the music folder, and unfinished tracks can be resumed on the next launch.

---

### User Story 2 - Review and fix YouTube matches (Priority: P2)

For tracks imported from Spotify or SoundCloud, the user can see which YouTube video was chosen,
how confident the match is, and replace it with another candidate or a pasted YouTube link. Tracks
with low-confidence matches are highlighted before the download starts.

**Why this priority**: Automatic matching will sometimes pick a live version, a cover or the wrong
song. Without a way to fix this, the library fills with wrong audio.

**Independent Test**: Import a playlist that contains a track with a known ambiguous title, open its
match, pick a different candidate, and confirm that the downloaded file is the chosen version.

**Acceptance Scenarios**:

1. **Given** an imported non-YouTube playlist, **When** matching completes, **Then** each track shows
   its chosen YouTube source and a match confidence (e.g. high / medium / low).
2. **Given** a low-confidence match, **When** the user opens it, **Then** the app shows at least 3
   alternative candidates with title, channel and duration.
3. **Given** a track the user re-matched, **When** it is downloaded, **Then** the user's chosen source
   is used, and the choice is remembered on later re-imports.

---

### User Story 3 - Browse and search the library (Priority: P3)

The user browses their local library by tracks, artists, albums and playlists, with artwork, and can
search instantly across title, artist and album. The interface is polished, visually appealing and
smooth, with light and dark themes.

**Why this priority**: Once music is downloaded, the user needs a pleasant way to find and look
through it. This is where the "beautiful and smooth" experience matters most.

**Independent Test**: With a library of at least 1,000 tracks, browse each view, scroll quickly,
search for a known track, and confirm that the results and artwork appear without visible lag.

**Acceptance Scenarios**:

1. **Given** a library with downloaded tracks, **When** the user opens the Artists view, **Then**
   every artist is listed, and selecting one shows their albums and tracks with artwork.
2. **Given** the search box, **When** the user types part of a title, artist or album, **Then**
   matching results update as they type.
3. **Given** a track in the library, **When** the user chooses "Show in folder" or "Open",
   **Then** the operating system reveals the file or opens it in the default player.
4. **Given** a folder of existing music files, **When** the user adds it to the library, **Then**
   its tracks appear with their existing tags and artwork, and the files are left unchanged.
5. **Given** the operating system is set to dark mode, **When** the app starts, **Then** it uses the
   dark theme automatically, and the user can override this choice.

---

### User Story 4 - Organize the library (Priority: P4)

The user edits track tags (title, artist, album, artwork), creates and edits local playlists, and
reorganizes files on disk using a naming pattern (e.g. `Artist/Album/NN - Title`). Every bulk
change shows a preview first, and nothing is deleted without confirmation.

**Why this priority**: Organizing is valuable, but the library first has to exist (P1) and be
browsable (P3).

**Independent Test**: Edit the tags of one track, create a playlist from 5 tracks, apply a folder
naming pattern to 20 tracks, and confirm the previewed result matches what is on disk afterwards.

**Acceptance Scenarios**:

1. **Given** a track, **When** the user edits its tags and saves, **Then** both the library and the
   file's embedded tags are updated.
2. **Given** a selection of tracks, **When** the user applies a naming pattern, **Then** a preview
   lists every old and new path before anything is moved, and the user can cancel.
3. **Given** a requested deletion, **When** the user confirms, **Then** the file is moved to the
   operating system's trash or recycle bin, not permanently deleted.
4. **Given** a local playlist, **When** the user exports it, **Then** a standard playlist file (e.g.
   M3U) is produced that other music players can open.

---

### User Story 5 - Keep imported playlists in sync (Priority: P5)

The user re-syncs a previously imported playlist. The app detects tracks that were added to (or
removed from) the online playlist and downloads only the new ones.

**Why this priority**: This is a convenience on top of P1, useful for playlists that keep growing.

**Independent Test**: Import a playlist, add 2 tracks to it on the platform, re-sync, and confirm
that only those 2 tracks are downloaded.

**Acceptance Scenarios**:

1. **Given** an imported playlist with new tracks online, **When** the user re-syncs, **Then** only
   the new tracks are matched and downloaded.
2. **Given** tracks that were removed from the online playlist, **When** the user re-syncs, **Then**
   the app shows them as removed online but keeps the local files unless the user chooses
   otherwise.

---

### Edge Cases

- The playlist link is invalid, private, deleted or from an unsupported platform: the app explains
  why it cannot be imported and does not crash.
- The network drops mid-import: completed tracks stay done, and the rest pause and can be retried.
- A platform rate-limits or temporarily blocks requests: the app slows down and retries, and shows
  that it is waiting instead of failing silently.
- A YouTube video is age-restricted, region-blocked or removed: the track is marked failed with
  that reason, and the user can pick another source.
- Track titles contain characters that are invalid in file names on some operating systems (e.g.
  `/ : * ? " < > |`): file names are sanitized consistently on Windows, Linux and macOS.
- Very long names that exceed path-length limits (notably on Windows): they are shortened safely.
- The music folder is on a disk that becomes full or is disconnected: downloads stop with a clear
  message, and no corrupt files remain.
- Very large playlists (1,000+ tracks): the import stays responsive and shows progress.
- The same song appears in several imported playlists: it is stored once and referenced by each
  playlist.
- A platform sign-in expires or is revoked: the app asks the user to reconnect, and public-link
  imports keep working.
- A scanned folder contains unsupported or corrupt audio files: they are skipped and listed, and
  the scan continues.
- Files in the music folder are changed or deleted outside the app: the library detects this on
  the next scan and does not show broken entries.

## Requirements *(mandatory)*

### Functional Requirements

**Import & platforms**

- **FR-001**: Users MUST be able to import a playlist, album or single track by pasting a link from
  Spotify, YouTube or SoundCloud.
- **FR-002**: The app MUST recognize which platform a link belongs to automatically and reject
  unsupported links with a clear message.
- **FR-003**: The app MUST be designed so that support for additional platforms can be added without
  changing how import, matching, download or library features work.
- **FR-004**: Users MUST be able to import public playlists by link without signing in. Users MAY
  also connect their Spotify, YouTube and SoundCloud accounts to import private playlists and their
  "Liked songs" or equivalent collection.
- **FR-004a**: Users MUST be able to see which accounts are connected and disconnect any of them at
  any time. Disconnecting removes the stored credentials from the computer.
- **FR-004b**: Importing someone else's public Spotify playlist by link is best-effort. If it stops
  working because Spotify changed its public pages, the app MUST explain this and suggest signing in
  and copying the playlist into the user's own account.
- **FR-004c**: Connecting an account MAY require the user to register their own free developer
  application with that platform once. The app MUST guide the user through this step by step.

**Matching**

- **FR-005**: For tracks that do not come from YouTube, the app MUST automatically find the best
  matching YouTube audio source using title, artists and duration.
- **FR-006**: Each match MUST show a confidence level, and low-confidence matches MUST be highlighted
  before the download starts.
- **FR-007**: Users MUST be able to replace a match with another suggested candidate or a pasted
  YouTube link, and the app MUST remember the choice.

**Download & tagging**

- **FR-008**: The app MUST download audio for each track into the user-chosen music folder.
- **FR-009**: Downloaded files MUST have embedded tags for title, artists, album, track number when
  known, and cover artwork when available.
- **FR-010**: Users MUST be able to choose the audio format from a small set of common formats, with
  a sensible default that keeps the original quality.
- **FR-011**: Downloads MUST run in the background with per-track and overall progress, and users
  MUST be able to pause, resume, cancel and retry failed tracks.
- **FR-012**: A failed track MUST NOT stop the rest of the batch.
- **FR-013**: Interrupted downloads MUST NOT leave partial files in the music folder.
- **FR-014**: Importing the same playlist or track again MUST NOT create duplicate files or
  duplicate library entries.

**Library**

- **FR-015**: The library MUST include both tracks downloaded by the app and existing audio files
  from folders the user adds. When scanning, the app MUST read the embedded tags and artwork of the
  existing files and MUST NOT modify them unless the user edits them.
- **FR-015a**: Users MUST be able to add and remove watched music folders. Removing a folder removes
  its tracks from the library but never deletes the files.
- **FR-015b**: When a download matches a song already in the library (including scanned files), the
  app MUST recognize the existing track instead of downloading a duplicate.
- **FR-016**: Users MUST be able to browse the library by tracks, artists, albums and playlists, with
  artwork.
- **FR-017**: Users MUST be able to search the library by title, artist and album, with results
  updating as they type.
- **FR-018**: Users MUST be able to reveal a track in the system file manager and open it in the
  default system player.
- **FR-019**: The library MUST detect files that were changed, moved or deleted outside the app.

**Organize**

- **FR-020**: Users MUST be able to edit a track's tags (title, artists, album, track number,
  artwork), and the changes MUST be written to the file.
- **FR-021**: Users MUST be able to create, rename, reorder and delete local playlists, and export
  them in a standard playlist format.
- **FR-022**: Users MUST be able to rename and move files using a configurable naming pattern, with a
  preview of every change before it is applied.
- **FR-023**: Deleting tracks MUST require confirmation and MUST move files to the system trash or
  recycle bin.
- **FR-024**: Users MUST be able to re-sync an imported playlist so that only new tracks are
  downloaded, with tracks removed online flagged rather than deleted.

**Experience & platform**

- **FR-025**: The app MUST run on Windows, Linux and macOS with the same features.
- **FR-026**: The interface MUST be visually polished and consistent, support light and dark themes
  (following the system by default), and use smooth transitions and animations.
- **FR-027**: The interface MUST stay responsive during imports, downloads and library scans.
- **FR-028**: Any platform credentials MUST be stored securely on the user's computer and never be
  shown in logs.
- **FR-029**: Browsing, searching and organizing the existing library MUST work offline.

### Key Entities

- **Track**: A song in the library. Title, artists, album, track number, duration, artwork, audio
  file location, format, and download status. Linked to its sources and to any playlists.
- **Artist** / **Album**: Groupings derived from track metadata, used for browsing, with album
  artwork.
- **Platform**: An external music service (Spotify, YouTube, SoundCloud, and future platforms) that
  playlists and tracks can be imported from.
- **Source Reference**: Where a track came from: the platform, its ID or link, and the original
  metadata as reported by that platform.
- **Match**: The link between a non-YouTube track and the YouTube source chosen for its audio. It
  includes the confidence, the alternative candidates, and whether it was picked automatically or
  by the user.
- **Imported Playlist**: A playlist imported from a platform, with its link, name, ordered tracks,
  and last sync time. Can be re-synced.
- **Local Playlist**: A playlist the user creates and orders in the app. Can be exported.
- **Download Job**: One track's download attempt: its status, progress, failure reason and number of
  retries. Grouped into a batch per import.
- **Connected Account**: A user's sign-in to a platform, used to read private playlists. Holds a
  reference to securely stored credentials, never the credentials themselves in plain form.
- **Watched Folder**: A folder on disk whose audio files are scanned into the library.
- **Settings**: Music folder, audio format, naming pattern, theme, and connected platform accounts.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A user can go from pasting a 50-track playlist link to all tracks downloading in under
  1 minute of interaction, with no manual steps per track.
- **SC-002**: At least 90% of tracks imported from Spotify or SoundCloud are matched to the correct
  song (same recording, not a cover or wrong song) without user correction.
- **SC-003**: 100% of successfully downloaded tracks contain title, artist and album tags, and 95%
  contain artwork.
- **SC-004**: A 500-track playlist import finishes with every track in a final state (done, skipped
  or failed with a reason); no track is left stuck or silently missing.
- **SC-005**: Re-importing a playlist creates zero duplicate files.
- **SC-006**: In a library of 10,000 tracks, search results appear within 0.5 seconds of typing and
  scrolling stays visually smooth, with no stutter.
- **SC-007**: The app opens to a usable library view within 3 seconds on a typical laptop.
- **SC-008**: Every core user story can be completed on Windows, Linux and macOS with identical
  results.
- **SC-009**: No user file is ever lost or corrupted by the app. Interrupted downloads and cancelled
  bulk changes leave the library exactly as it was before.
- **SC-010**: Scanning a folder of 5,000 existing audio files adds them to the library within 2
  minutes, without modifying any file.

## Assumptions

- The app is for a single user on their own computer. There are no shared accounts, sync between
  devices, or server.
- Audio is always obtained from YouTube. Spotify and SoundCloud provide only playlist and track
  information.
- The user is responsible for downloading only content they are allowed to, for personal use. The
  app does not bypass DRM or platform login protections.
- Platform API rules (as of 2026-10) restrict hobby apps: Spotify lets an app read the track list
  only of playlists the signed-in user owns or collaborates on, and requires the developer account
  owner to have Spotify Premium. Signed-in features therefore assume the user has Premium and
  registers their own Spotify developer app. SoundCloud sign-in similarly needs the user's own
  developer app, which requires an Artist Pro account.
- In-app music playback is out of scope for this feature. Tracks are opened in the system's default
  player.
- Re-syncing playlists is manual (the user clicks re-sync). Automatic background syncing is out of
  scope.
- Default file layout is `Artist/Album/NN - Title`, which the user can change.
- The user's computer has internet access during import and download, but not for browsing the
  library.
- The visual "beautiful and smooth" quality will be judged by the user against the success criteria
  above. Specific design and technology choices are made in planning.
