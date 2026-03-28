package source

import "time"

// Track is the resolved, download-ready representation of a single audio item.
// For YouTube tracks, URL and Title are set.
// For Spotify tracks, Title and Artist are set; the downloader searches YouTube at download time.
type Track struct {
	Title    string
	Artist   string
	URL      string
	Duration time.Duration // 0 if unknown
}

// DisplayName returns a concise label for TUI rendering.
func (t Track) DisplayName() string {
	if t.Artist != "" {
		return t.Artist + " — " + t.Title
	}
	if t.Title != "" {
		return t.Title
	}
	return t.URL
}

// Source resolves an input URL into a flat list of Tracks.
type Source interface {
	Resolve(url string) ([]Track, error)
}
