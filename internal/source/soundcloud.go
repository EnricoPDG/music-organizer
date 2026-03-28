package source

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"regexp"
)

// SoundCloudSource resolves SoundCloud track and playlist URLs into Tracks.
// Uses the unofficial SoundCloud API v2 with a client_id extracted from their JS.
type SoundCloudSource struct{}

func NewSoundCloudSource() *SoundCloudSource { return &SoundCloudSource{} }

func (s *SoundCloudSource) Resolve(rawURL string) ([]Track, error) {
	clientID, err := fetchSoundCloudClientID()
	if err != nil {
		return nil, fmt.Errorf("could not get SoundCloud client_id: %w", err)
	}
	return resolveSoundCloudURL(rawURL, clientID)
}

// fetchSoundCloudClientID extracts the client_id from SoundCloud's bundled JS.
func fetchSoundCloudClientID() (string, error) {
	req, _ := http.NewRequest("GET", "https://soundcloud.com", nil)
	req.Header.Set("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")

	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()

	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return "", err
	}

	// Find asset script URLs embedded in the page.
	scriptRe := regexp.MustCompile(`<script[^>]+src="(https://a-v2\.sndcdn\.com/assets/[^"]+\.js)"`)
	matches := scriptRe.FindAllSubmatch(body, -1)

	for _, m := range matches {
		id, err := extractClientIDFromScript(string(m[1]))
		if err == nil && id != "" {
			return id, nil
		}
	}

	return "", fmt.Errorf("client_id not found in SoundCloud scripts")
}

func extractClientIDFromScript(scriptURL string) (string, error) {
	req, _ := http.NewRequest("GET", scriptURL, nil)
	req.Header.Set("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")

	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()

	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return "", err
	}

	re := regexp.MustCompile(`client_id:"([a-zA-Z0-9]{32})"`)
	m := re.FindSubmatch(body)
	if m == nil {
		return "", fmt.Errorf("client_id not found")
	}
	return string(m[1]), nil
}

// ── API resolution ────────────────────────────────────────────────────────────

func resolveSoundCloudURL(rawURL, clientID string) ([]Track, error) {
	apiURL := "https://api-v2.soundcloud.com/resolve?url=" + url.QueryEscape(rawURL) + "&client_id=" + clientID

	req, _ := http.NewRequest("GET", apiURL, nil)
	req.Header.Set("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")

	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("SoundCloud API returned %s", resp.Status)
	}

	var resource map[string]any
	if err := json.NewDecoder(resp.Body).Decode(&resource); err != nil {
		return nil, fmt.Errorf("decode SoundCloud response: %w", err)
	}

	switch resource["kind"] {
	case "track":
		t := scTrackToTrack(resource)
		return []Track{t}, nil
	case "playlist":
		return scPlaylistToTracks(resource, clientID)
	default:
		return nil, fmt.Errorf("unsupported SoundCloud resource kind: %v", resource["kind"])
	}
}

func scTrackToTrack(m map[string]any) Track {
	title, _ := m["title"].(string)

	var artist string
	if user, ok := m["user"].(map[string]any); ok {
		artist, _ = user["username"].(string)
	}

	// Duration intentionally omitted — the actual download comes from YouTube
	// search, so the SoundCloud duration wouldn't match anyway.
	return Track{Title: title, Artist: artist}
}

func scPlaylistToTracks(playlist map[string]any, clientID string) ([]Track, error) {
	rawTracks, ok := playlist["tracks"].([]any)
	if !ok {
		return nil, fmt.Errorf("no tracks in SoundCloud playlist")
	}

	var tracks []Track
	var missingIDs []string

	for _, item := range rawTracks {
		m, ok := item.(map[string]any)
		if !ok {
			continue
		}
		// Playlist items may be stubs with only an id — fetch full data only if needed.
		if _, hasTitle := m["title"].(string); hasTitle && m["title"] != "" {
			tracks = append(tracks, scTrackToTrack(m))
		} else if id, ok := m["id"].(float64); ok {
			missingIDs = append(missingIDs, fmt.Sprintf("%.0f", id))
		}
	}

	// Fetch missing track metadata in batches of 50.
	for i := 0; i < len(missingIDs); i += 50 {
		end := i + 50
		if end > len(missingIDs) {
			end = len(missingIDs)
		}
		batch := missingIDs[i:end]

		ids := ""
		for j, id := range batch {
			if j > 0 {
				ids += ","
			}
			ids += id
		}

		batchURL := "https://api-v2.soundcloud.com/tracks?ids=" + ids + "&client_id=" + clientID
		req, _ := http.NewRequest("GET", batchURL, nil)
		req.Header.Set("User-Agent", "Mozilla/5.0")

		resp, err := http.DefaultClient.Do(req)
		if err != nil {
			continue
		}

		var batchTracks []map[string]any
		if err := json.NewDecoder(resp.Body).Decode(&batchTracks); err != nil {
			resp.Body.Close()
			continue
		}
		resp.Body.Close()

		for _, t := range batchTracks {
			tracks = append(tracks, scTrackToTrack(t))
		}
	}

	return tracks, nil
}
