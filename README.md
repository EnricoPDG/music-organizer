# Music Organizer

> Open-source study project by [Enrico Papsch Di Giacomo](https://github.com/EnricoPDG), built with
> the help of [Claude](https://claude.ai) (Anthropic).

A desktop app for macOS, Windows and Linux that turns playlists into a local music library. Paste
a Spotify, YouTube or SoundCloud link: the app finds each song on YouTube, downloads the audio and
saves it as a tagged file (title, artist, album, cover art) in your music folder.

Version 2 is a rewrite of the [v1 Go/Wails app](https://github.com/EnricoPDG/music-organizer/tree/v1.0.1)
with Tauri 2 (Rust) and Svelte 5.

## Download

Get the installer for your system from the [latest release](https://github.com/EnricoPDG/music-organizer/releases/latest):

| System                          | File                                                                |
| ------------------------------- | ------------------------------------------------------------------- |
| macOS (Apple Silicon and Intel) | `Music.Organizer_<version>_universal.dmg`                           |
| Windows 10/11                   | `Music.Organizer_<version>_x64_en-US.msi` or `…_x64-setup.exe`      |
| Linux                           | `.AppImage` (any distro), `.deb` (Ubuntu/Debian) or `.rpm` (Fedora) |

The builds are not code-signed yet, so your system will warn you the first time:

- **macOS:** if it says the app "is damaged" or "can't be opened", run
  `xattr -cr "/Applications/Music Organizer.app"` in Terminal once, then open it again.
- **Windows:** on the SmartScreen prompt, choose **More info → Run anyway**.
- **Linux (AppImage):** make it executable with `chmod +x Music.Organizer_*.AppImage`.
  Signing in to accounts needs a Secret Service provider (GNOME Keyring or KWallet).

## What works in 2.0.0

- Import a playlist, album or track by link from Spotify, YouTube or SoundCloud, with a preview.
- Automatic matching of Spotify and SoundCloud tracks to YouTube.
- Tagged downloads (M4A by default; Opus and MP3 need FFmpeg installed) into
  `Artist/Album/NN - Title`, with a live download list: pause, resume, cancel, retry.
- Re-importing never creates duplicates; interrupted downloads never leave broken files.
- Optional sign-in to import private playlists and liked songs, using your own free developer app
  for each platform (the app walks you through it). Sign-ins are kept in your system's credential
  store.
- Light and dark themes.

Coming next: reviewing and fixing matches, browsing and searching the library, organizing files,
and re-syncing playlists. See [`specs/001-music-library-organizer`](specs/001-music-library-organizer/).

Only download content you're allowed to, for personal use.

## Development

Requirements: Rust (stable), Node.js 22+, pnpm 10, and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your system.

```bash
pnpm install
pnpm tauri dev      # run the app
pnpm tauri build    # build installers into src-tauri/target/release/bundle/
```

Checks: `pnpm check`, `pnpm lint`, `pnpm test`, `pnpm test:e2e`, and `cargo test` in `src-tauri/`.

To publish a release, push a tag such as `v2.0.1`. GitHub Actions builds the installers for all
three systems and creates a draft release to review and publish.

## License

MIT, see [LICENSE](LICENSE).
