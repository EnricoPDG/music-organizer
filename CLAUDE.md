# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

A native desktop app (macOS/Windows/Linux) built with Wails v2 that batch-downloads audio from YouTube and Spotify as `.m4a` files. Go backend + vanilla HTML/CSS/JS frontend compiled into a single binary.

## External Dependencies

None required at runtime. Everything is pure Go.

## Commands

**Dev mode (hot-reload):**
```bash
wails dev
```

**Production build:**
```bash
wails build
```

## Architecture

```
main.go              # Wails entry point — embeds frontend/, wires App
app.go               # Backend: all exported methods callable from JS via window.go.main.App.*
wails.json           # Wails project config (no npm/bundler, vanilla JS)
frontend/
  index.html         # Three-step UI: URL input → Spotify credentials → tracks + download
  style.css          # Dark theme, gold (#ffdb89) accent
  script.js          # Wails IPC calls + event listeners
  wailsjs/           # Auto-generated Wails JS bindings (do not edit)
build/               # App icons and platform manifests
internal/
  source/
    source.go        # Track struct + Source interface
    youtube.go       # YouTubeSource: innertube API for playlists, kkdai/youtube for single videos
    spotify.go       # SpotifySource: Spotify API → Track{Title, Artist, Duration}
  downloader/
    downloader.go    # Run(): semaphore pool (cap=5), streams m4a to disk
```

## Key Design Points

- **Wails IPC**: JS calls Go methods via `window.go.main.App.MethodName()`. Progress events are streamed back with `runtime.EventsEmit` → `window.runtime.EventsOn`.
- **YouTube resolution**: playlist URLs use a custom innertube API caller (avoids `kkdai/youtube` panic on videos with empty `lengthSeconds`). Single videos use `client.GetVideo()`.
- **Spotify** uses Client Credentials flow (no user OAuth). Tracks have `Title` + `Artist`; the downloader searches YouTube at download time via `searchYouTube()`.
- **Download** selects the highest-bitrate `audio/mp4` stream and streams it directly to a `.m4a` file.
- **Concurrency** is a buffered channel semaphore (`cap=5`) in `downloader.Run`.
- **Native directory picker**: `App.PickDirectory()` calls `runtime.OpenDirectoryDialog` — no custom modal needed.
