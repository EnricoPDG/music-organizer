package source

import (
	"bytes"
	"encoding/json"
	"fmt"
	"net/http"
	"net/url"
	"strings"

	"github.com/kkdai/youtube/v2"

	"music-organizer/internal/jsonutil"
)

// YouTubeSource handles single video URLs and playlist URLs.
type YouTubeSource struct{}

func NewYouTubeSource() *YouTubeSource { return &YouTubeSource{} }

func (y *YouTubeSource) Resolve(rawURL string) ([]Track, error) {
	if isPlaylistURL(rawURL) {
		// Use our own fetcher to avoid a panic inside kkdai/youtube when a
		// playlist entry has an empty lengthSeconds (live/upcoming/private videos).
		return fetchPlaylist(rawURL)
	}

	client := youtube.Client{}
	video, err := client.GetVideo(rawURL)
	if err != nil {
		return nil, fmt.Errorf("could not get video: %w", err)
	}
	return []Track{{Title: video.Title, URL: rawURL, Duration: video.Duration}}, nil
}

// isPlaylistURL returns true for any YouTube URL with a list= parameter.
func isPlaylistURL(rawURL string) bool {
	u, err := url.Parse(rawURL)
	if err != nil {
		return false
	}
	return strings.Contains(u.Host, "youtube.com") && u.Query().Get("list") != ""
}

// ── Playlist fetcher ──────────────────────────────────────────────────────────
//
// We call the YouTube innertube browse API directly so we can skip entries
// with invalid/missing duration instead of panicking like the library does.

const (
	innertubeKey     = "AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Y9_11qcW8"
	innertubeURL     = "https://www.youtube.com/youtubei/v1/browse?key=" + innertubeKey
	innertubeClient  = "WEB"
	innertubeVersion = "2.20220801.00.00"
)

func fetchPlaylist(rawURL string) ([]Track, error) {
	u, _ := url.Parse(rawURL)
	playlistID := u.Query().Get("list")
	if playlistID == "" {
		return nil, fmt.Errorf("no list= parameter in URL")
	}

	var tracks []Track
	token := ""
	for {
		batch, next, err := innertubePlaylistPage(playlistID, token)
		if err != nil {
			return nil, err
		}
		tracks = append(tracks, batch...)
		if next == "" {
			break
		}
		token = next
	}
	return tracks, nil
}

func innertubePlaylistPage(playlistID, continuation string) ([]Track, string, error) {
	ctx := map[string]any{
		"client": map[string]any{
			"clientName":    innertubeClient,
			"clientVersion": innertubeVersion,
			"hl":            "en",
			"gl":            "US",
			"timeZone":      "UTC",
		},
	}

	var reqBody map[string]any
	if continuation == "" {
		reqBody = map[string]any{
			"browseId":       "VL" + playlistID,
			"context":        ctx,
			"contentCheckOk": true,
			"racyCheckOk":    true,
		}
	} else {
		reqBody = map[string]any{
			"continuation":   continuation,
			"context":        ctx,
			"contentCheckOk": true,
			"racyCheckOk":    true,
		}
	}

	buf, _ := json.Marshal(reqBody)
	resp, err := http.Post(innertubeURL, "application/json", bytes.NewReader(buf))
	if err != nil {
		return nil, "", err
	}
	defer resp.Body.Close()

	var body map[string]any
	if err := json.NewDecoder(resp.Body).Decode(&body); err != nil {
		return nil, "", fmt.Errorf("decode innertube response: %w", err)
	}

	items := extractPlaylistItems(body, continuation != "")
	return parsePlaylistItems(items)
}

// extractPlaylistItems navigates the innertube JSON to find the flat list of
// content items, which differs between initial and continuation responses.
func extractPlaylistItems(body map[string]any, isContinuation bool) []any {
	if isContinuation {
		// onResponseReceivedActions[0].appendContinuationItemsAction.continuationItems
		actions := jsonutil.Idx(jsonutil.Get(body, "onResponseReceivedActions"), 0)
		raw := jsonutil.Get(actions, "appendContinuationItemsAction", "continuationItems")
		items, _ := raw.([]any)
		return items
	}

	// contents.twoColumnBrowseResultsRenderer
	//   .tabs[0].tabRenderer.content.sectionListRenderer
	//   .contents[0].itemSectionRenderer.contents[0]
	//   .playlistVideoListRenderer.contents
	path := jsonutil.Get(body, "contents", "twoColumnBrowseResultsRenderer")
	tab := jsonutil.Idx(jsonutil.Get(path, "tabs"), 0)
	sl := jsonutil.Get(tab, "tabRenderer", "content", "sectionListRenderer")
	section := jsonutil.Idx(jsonutil.Get(sl, "contents"), 0)
	listRenderer := jsonutil.Idx(jsonutil.Get(section, "itemSectionRenderer", "contents"), 0)
	raw := jsonutil.Get(listRenderer, "playlistVideoListRenderer", "contents")
	items, _ := raw.([]any)
	return items
}

// parsePlaylistItems converts raw JSON items to Tracks, skipping invalid entries.
func parsePlaylistItems(items []any) ([]Track, string, error) {
	var tracks []Track
	var nextToken string

	for _, item := range items {
		m, ok := item.(map[string]any)
		if !ok {
			continue
		}

		if renderer, ok := m["playlistVideoRenderer"].(map[string]any); ok {
			videoID, _ := renderer["videoId"].(string)
			if videoID == "" {
				continue // skip private / unavailable videos
			}
			tracks = append(tracks, Track{
				Title: jrunText(renderer["title"]),
				URL:   "https://www.youtube.com/watch?v=" + videoID,
				// Duration intentionally omitted — not reliable for all entry types.
			})
		}

		if cont, ok := m["continuationItemRenderer"].(map[string]any); ok {
			if t, ok := jsonutil.Get(cont, "continuationEndpoint", "continuationCommand", "token").(string); ok {
				nextToken = t
			}
		}
	}

	return tracks, nextToken, nil
}

// jrunText extracts text from a YouTube {runs:[{text:"..."}]} node.
func jrunText(v any) string {
	runs, _ := jsonutil.Get(v, "runs").([]any)
	if len(runs) == 0 {
		return ""
	}
	text, _ := jsonutil.Get(runs[0], "text").(string)
	return text
}

// ── Search ────────────────────────────────────────────────────────────────────

const innertubeSearchURL = "https://www.youtube.com/youtubei/v1/search?key=" + innertubeKey

// Search queries YouTube and returns up to limit video results.
func Search(query string, limit int) ([]Track, error) {
	body, _ := json.Marshal(map[string]any{
		"context": map[string]any{
			"client": map[string]any{
				"clientName":    innertubeClient,
				"clientVersion": innertubeVersion,
				"hl":            "en",
				"gl":            "US",
			},
		},
		"query": query,
	})

	resp, err := http.Post(innertubeSearchURL, "application/json", bytes.NewReader(body))
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	var result map[string]any
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, fmt.Errorf("decode search response: %w", err)
	}

	// Navigate to the video renderer list.
	primary := jsonutil.Get(result, "contents", "twoColumnSearchResultsRenderer", "primaryContents")
	section := jsonutil.Idx(jsonutil.Get(primary, "sectionListRenderer", "contents"), 0)
	items, _ := jsonutil.Get(section, "itemSectionRenderer", "contents").([]any)

	var tracks []Track
	for _, item := range items {
		m, ok := item.(map[string]any)
		if !ok {
			continue
		}
		renderer, ok := m["videoRenderer"].(map[string]any)
		if !ok {
			continue
		}
		videoID, _ := renderer["videoId"].(string)
		if videoID == "" {
			continue
		}
		title := jrunText(renderer["title"])
		channel := jrunText(jsonutil.Get(renderer, "ownerText"))
		tracks = append(tracks, Track{
			Title:  title,
			Artist: channel,
			URL:    "https://www.youtube.com/watch?v=" + videoID,
		})
		if len(tracks) >= limit {
			break
		}
	}

	if len(tracks) == 0 {
		return nil, fmt.Errorf("no results for: %s", query)
	}
	return tracks, nil
}
