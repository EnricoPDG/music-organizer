package downloader

import (
	"fmt"
	"io"
	"math/rand"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"time"

	"github.com/kkdai/youtube/v2"

	"music-organizer/internal/source"
)

// Result holds the outcome of a single track download.
type Result struct {
	Track source.Track
	Err   error
}

// Run downloads tracks sequentially with a small random delay between each
// to avoid triggering YouTube's bot detection (HTTP 429 / /sorry redirect).
func Run(tracks []source.Track, outputDir string, onResult func(Result)) {
	for i, t := range tracks {
		onResult(downloadOne(t, outputDir))
		if i < len(tracks)-1 {
			// 1–3 s jitter between requests
			time.Sleep(time.Duration(1000+rand.Intn(2000)) * time.Millisecond)
		}
	}
}

func downloadOne(t source.Track, outputDir string) Result {
	client := &youtube.Client{}

	videoURL := t.URL
	if videoURL == "" {
		id, err := searchYouTube(t.Artist + " - " + t.Title)
		if err != nil {
			return Result{Track: t, Err: fmt.Errorf("YouTube search failed: %w", err)}
		}
		videoURL = "https://www.youtube.com/watch?v=" + id
	}

	video, err := client.GetVideo(videoURL)
	if err != nil {
		return Result{Track: t, Err: err}
	}

	format := bestM4AFormat(video.Formats)
	if format == nil {
		return Result{Track: t, Err: fmt.Errorf("no m4a audio stream available")}
	}

	stream, _, err := client.GetStream(video, format)
	if err != nil {
		return Result{Track: t, Err: err}
	}
	defer stream.Close()

	name := video.Title
	if t.Artist != "" {
		name = t.Artist + " - " + t.Title
	}
	outPath := filepath.Join(outputDir, sanitizeFilename(name)+".m4a")

	f, err := os.Create(outPath)
	if err != nil {
		return Result{Track: t, Err: err}
	}
	defer f.Close()

	_, err = io.Copy(f, stream)
	return Result{Track: t, Err: err}
}

func bestM4AFormat(formats youtube.FormatList) *youtube.Format {
	var best *youtube.Format
	for i := range formats {
		f := &formats[i]
		if strings.HasPrefix(f.MimeType, "audio/mp4") {
			if best == nil || f.Bitrate > best.Bitrate {
				best = f
			}
		}
	}
	return best
}

func searchYouTube(query string) (string, error) {
	searchURL := "https://www.youtube.com/results?search_query=" + url.QueryEscape(query)
	req, err := http.NewRequest("GET", searchURL, nil)
	if err != nil {
		return "", err
	}
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

	re := regexp.MustCompile(`"videoId":"([a-zA-Z0-9_-]{11})"`)
	m := re.FindSubmatch(body)
	if m == nil {
		return "", fmt.Errorf("no results for: %s", query)
	}
	return string(m[1]), nil
}

func sanitizeFilename(name string) string {
	re := regexp.MustCompile(`[/\\:*?"<>|]`)
	return strings.TrimSpace(re.ReplaceAllString(name, "_"))
}
