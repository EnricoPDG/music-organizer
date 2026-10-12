# Quickstart & Validation Guide: Music Library Organizer

This guide shows how to run the app and how to check, end to end, that the feature works. Run the
validation scenarios on **Windows, Linux and macOS** (constitution, Definition of Done; SC-008).
Contracts: [ipc.md](./contracts/ipc.md), [platform-adapter.md](./contracts/platform-adapter.md).
Entities: [data-model.md](./data-model.md).

## Prerequisites

| Tool | Version | Notes |
|------|---------|-------|
| Rust | stable ≥ 1.85 (`rustup`) | |
| Node.js | 22 LTS + pnpm 10 | |
| Tauri system dependencies | see [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) | Linux: `libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, plus `libsecret`/gnome-keyring for credentials. Windows: WebView2 (preinstalled on Windows 10/11) and MSVC build tools. macOS: Xcode Command Line Tools |

You do **not** need to install yt-dlp, Deno or FFmpeg. The app installs them on first run
(research R4).

## Run

```bash
pnpm install
pnpm tauri dev          # development build with hot reload
pnpm tauri build        # installers in src-tauri/target/release/bundle/
```

## Automated checks (all offline)

```bash
cargo test --manifest-path src-tauri/Cargo.toml         # unit + adapter contract + matcher accuracy
pnpm test                                               # Vitest (stores, utils)
pnpm test:e2e                                           # Playwright against mocked IPC
pnpm check                                              # svelte-check + bindings up to date
```

Expected results:
- every test passes with the network disabled;
- the matcher accuracy report shows **≥ 90%** correct on the labelled fixture set (SC-002).

Optional live smoke checks: `cargo test --features live -- --ignored`. These need network access
and are never run in CI.

## Validation scenarios

Use a **fresh profile**: an empty app data folder and a new, empty music folder.

### V1. First run & tools (R4)
1. Launch the app → onboarding asks for a music folder → the app downloads its tools with a progress
   bar.
2. ✅ Settings → Tools shows yt-dlp, Deno and FFmpeg installed with their versions.

### V2. Import a public YouTube playlist (US1, FR-001/008/009/011)
1. Paste a public YouTube playlist link (10–20 videos) → a preview card shows the name, owner and
   track count.
2. Click **Import & download**.
3. ✅ The Queue shows each track moving through queued → downloading → done, and the UI stays
   responsive (you can scroll the library while it downloads).
4. ✅ Files appear as `Artist - Title.m4a` in the music folder. Opening one in the system player shows the
   title, artist and artwork (SC-003).

### V3. Import Spotify & SoundCloud by public link (US1, US2, FR-004b, FR-005/006)
1. Paste a public Spotify playlist link that you **don't** own → the preview shows the
   "best-effort public import" label (`fetchMode = public_fallback`).
2. Paste a public SoundCloud set link.
3. ✅ Each track shows its chosen YouTube source with a high/medium/low badge. Low-confidence
   tracks are listed first in Match review.
4. Open a low-confidence track → ✅ at least 3 candidates are shown. Pick another one → ✅ the file
   is re-downloaded from the chosen video, and the choice is still there after re-importing.

### V4. Idempotency & resilience (FR-012/013/014, SC-004/005/009)
1. Import the same YouTube playlist again → ✅ every track is `skipped`, and no new files appear.
2. Start a 30-track import, then disconnect the network halfway → ✅ the remaining jobs pause with a
   "waiting for network" status. Reconnect → ✅ they resume.
3. Quit the app during a download → relaunch → ✅ no `.part` files remain in the music folder, and
   unfinished jobs are queued again.
4. Include a removed or private video in a playlist → ✅ that track is `failed` with a readable
   reason, and all the others finish.

### V5. Sign in (FR-004, FR-004a/c)
1. Settings → Accounts → Spotify → follow the guided setup (create your own Spotify developer app,
   paste the client ID) → the browser opens, you approve, and you return to the app.
2. ✅ Import shows "My playlists" and **Liked songs**. Importing your own playlist uses
   `fetchMode = official`.
3. Disconnect → ✅ the account disappears, and the OS credential store no longer has the entry
   (macOS Keychain Access / Windows Credential Manager / `secret-tool search service music-organizer`).
4. Repeat for YouTube (Google Cloud OAuth desktop client).

### V6. Scan existing music (FR-015/015a/015b, SC-010)
1. Add a folder containing existing audio files (ideally 1,000+, in mixed formats).
2. ✅ The tracks appear with their tags and artwork. The files' modification times are **unchanged**
   (compare `ls -l` / Explorer before and after).
3. Import a playlist containing a song you already have in that folder → ✅ that track is `skipped`
   as already in the library.
4. Remove the folder from the library → ✅ its tracks disappear from the app, and the files are
   still on disk.

### V7. Browse, search & polish (US3, FR-016–018, FR-026/027, SC-006/007)
1. With 10,000 or more tracks (use V6 with a large folder), open Tracks, Artists and Albums, and
   scroll quickly.
2. ✅ No visible stutter. Search results update as you type and appear within 0.5 s.
3. ✅ A cold start reaches the library view within 3 s.
4. Switch the OS between dark and light mode → ✅ the app follows. Turn on reduced motion in the OS
   → ✅ animations are reduced.
5. Track context menu → **Show in folder** / **Open** → ✅ the OS file manager or default player
   opens.

### V8. Organize (US4, FR-020–023)
1. Edit the title and artwork of one track → ✅ the change shows in the app and in the system
   player.
2. Select 20 tracks → apply the pattern `{artist}/{title}` → ✅ a preview table lists every old and
   new path, with conflicts highlighted. Click **Cancel** → ✅ nothing has changed on disk.
3. Apply it → ✅ the files move as previewed. Click **Undo** → ✅ they are restored.
4. Use titles containing `: ? * "` and a very long album name (on Windows) → ✅ the names are
   sanitized and shortened consistently, with no errors.
5. Delete a track → you are asked to confirm → ✅ the file is in the OS trash or recycle bin.
6. Create a local playlist → export it as M3U8 → ✅ it opens in VLC or another player.

### V9. Re-sync (US5, FR-024)
1. Add 2 tracks to (and remove 1 from) an imported playlist on the platform → click **Re-sync**.
2. ✅ Only the 2 new tracks are downloaded. The removed track is labelled "removed online", and its
   file is kept.

## Done when

- V1–V9 pass on Windows, Linux and macOS;
- the CI matrix is green;
- no secrets appear in the logs (`<app log dir>/*.log` contains no tokens; check with `grep -i token`).
