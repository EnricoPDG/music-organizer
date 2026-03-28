# Music Organizer

> **Open-source study project** by [Enrico Papsch Di Giacomo](https://github.com/EnricoPDG)
> Built as a personal learning exercise using **Go** as the primary language, developed with the assistance of [Claude](https://claude.ai) (Anthropic).

A native desktop app for macOS, Windows, and Linux that batch-downloads audio from **YouTube**, **Spotify**, and **SoundCloud** as `.m4a` files.

Built with [Wails v2](https://wails.io) — Go backend + Svelte frontend compiled into a single binary. No `yt-dlp`, no `ffmpeg` required.

## About this project

This repository is a hands-on study of:

- **Go** — backend logic, HTTP clients, concurrency, and the standard library
- **Wails v2** — bridging a Go backend with a web frontend into a native desktop app
- **Claude (AI)** — used as a coding assistant throughout the entire development process

The goal is learning by building something real and useful. The code is intentionally kept simple and readable.

## Features

- Paste a YouTube video or playlist URL → download directly as `.m4a`
- Paste a Spotify track, album, or playlist → searches and downloads matching audio from YouTube
- Paste a SoundCloud track or set URL → resolves metadata and downloads via YouTube search
- Duration filter to skip intros, interludes, or bonus tracks
- Native folder picker for choosing the output directory
- Sequential downloads with jitter to avoid rate-limiting

## Prerequisites

- [Go 1.21+](https://go.dev/dl/)
- [Wails v2 CLI](https://wails.io/docs/gettingstarted/installation): `go install github.com/wailsapp/wails/v2/cmd/wails@latest`
- Node.js (for the Svelte frontend build)

## Development

```bash
wails dev
```

## Production build

```bash
wails build
```

The output binary is placed in `build/bin/`.

## Tech stack

| Layer      | Technology                                                                |
|------------|---------------------------------------------------------------------------|
| Language   | **Go** (primary)                                                          |
| Desktop    | [Wails v2](https://wails.io)                                              |
| Frontend   | [Svelte](https://svelte.dev) + [Vite](https://vitejs.dev)                 |
| YouTube    | [kkdai/youtube](https://github.com/kkdai/youtube) + custom innertube API  |
| Spotify    | [zmb3/spotify](https://github.com/zmb3/spotify) (Client Credentials flow) |
| SoundCloud | Unofficial API v2 (no credentials needed)                                 |
| AI assist  | [Claude](https://claude.ai) by Anthropic                                  |

## License

MIT — see [LICENSE](LICENSE).

This project is open source and free to use for learning purposes.
