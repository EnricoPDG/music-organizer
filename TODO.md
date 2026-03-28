# TODO

Prioritized feature backlog for Music Organizer.

## Platform support
- [ ] Apple Music (requires MusicKit or scraping)
- [ ] Deezer
- [ ] Bandcamp
- [ ] YouTube Music playlists
- [ ] Tidal

## Download quality & metadata
- [ ] Format/quality selector (m4a, mp3, opus, best/medium/low)
- [ ] Embed ID3/MP4 metadata (title, artist, album, cover art)
- [ ] Download album artwork separately
- [ ] Preserve SoundCloud/Spotify track order in filenames (e.g. `01 - Track.m4a`)

## Download management
- [ ] Retry failed tracks (configurable attempts)
- [ ] Resume interrupted downloads
- [ ] Skip already-downloaded files (deduplication by filename)
- [ ] Configurable concurrency (currently sequential with jitter)
- [ ] Cancel in-progress download

## UI / UX
- [ ] Per-track checkboxes to deselect individual tracks before downloading
- [ ] Search/filter tracks by name within the resolved list
- [ ] Show download speed and ETA
- [ ] Desktop notification on completion
- [ ] Drag-and-drop URL onto the window
- [ ] Dark/light theme toggle

## Settings & persistence
- [ ] Remember last output directory across sessions
- [ ] Store Spotify credentials securely (OS keychain)
- [ ] Filename template (e.g. `{artist} - {title}`, `{title}`)
- [ ] Configurable max concurrency

## Misc
- [ ] Auto-update mechanism
- [ ] Localization (i18n)
- [ ] Error reporting / log export
