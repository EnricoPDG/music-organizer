package main

import (
	"context"
	"os"
	"path/filepath"
	"strings"

	keyring "github.com/zalando/go-keyring"
	log "github.com/sirupsen/logrus"
	"github.com/wailsapp/wails/v2/pkg/runtime"

	"music-organizer/internal/config"
	"music-organizer/internal/downloader"
	"music-organizer/internal/source"
)

const keyringSvc = "music-organizer"

// App is the Wails backend. Every exported method is callable from JS via
// window.go.main.App.MethodName().
type App struct {
	ctx context.Context
}

func NewApp() *App { return &App{} }

func (a *App) startup(ctx context.Context) {
	a.ctx = ctx
	log.WithField("version", "dev").Info("app started")
}

// ── Types ─────────────────────────────────────────────────────────────────────

type TrackDTO struct {
	Title       string `json:"title"`
	Artist      string `json:"artist"`
	URL         string `json:"url"`
	DurationSec int    `json:"durationSec"`
}

type ResolveResult struct {
	Tracks    []TrackDTO `json:"tracks,omitempty"`
	NeedsAuth bool       `json:"needsAuth,omitempty"`
	Error     string     `json:"error,omitempty"`
}

type SpotifyCredsDTO struct {
	ClientID     string `json:"clientId"`
	ClientSecret string `json:"clientSecret"`
}

// ── Output directory ──────────────────────────────────────────────────────────

// GetSavedOutputDir returns the last-used output directory, falling back to ~/Music.
func (a *App) GetSavedOutputDir() string {
	if dir := config.Load().OutputDir; dir != "" {
		return dir
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return "."
	}
	return filepath.Join(home, "Music")
}

// SaveOutputDir persists the chosen output directory for future sessions.
func (a *App) SaveOutputDir(dir string) {
	c := config.Load()
	c.OutputDir = dir
	config.Save(c)
}

// PickDirectory opens the native OS folder picker and returns the chosen path.
func (a *App) PickDirectory() string {
	home, _ := os.UserHomeDir()
	dir, err := runtime.OpenDirectoryDialog(a.ctx, runtime.OpenDialogOptions{
		DefaultDirectory: home,
		Title:            "Choose output directory",
	})
	if err != nil || dir == "" {
		return ""
	}
	return dir
}

// ── Spotify credentials (OS keychain) ────────────────────────────────────────

// GetSpotifyCredentials loads saved Spotify credentials from the OS keychain.
func (a *App) GetSpotifyCredentials() SpotifyCredsDTO {
	id, _ := keyring.Get(keyringSvc, "spotify-client-id")
	secret, _ := keyring.Get(keyringSvc, "spotify-client-secret")
	return SpotifyCredsDTO{ClientID: id, ClientSecret: secret}
}

// SaveSpotifyCredentials stores Spotify credentials in the OS keychain.
func (a *App) SaveSpotifyCredentials(clientID, clientSecret string) {
	keyring.Set(keyringSvc, "spotify-client-id", clientID)
	keyring.Set(keyringSvc, "spotify-client-secret", clientSecret)
}

// ── Resolve & Search ──────────────────────────────────────────────────────────

// Resolve resolves a YouTube, Spotify, or SoundCloud URL into a list of tracks.
func (a *App) Resolve(url, clientID, clientSecret string) ResolveResult {
	logger := log.WithField("url", url)

	var src source.Source
	if isSpotify(url) {
		if clientID == "" || clientSecret == "" {
			return ResolveResult{NeedsAuth: true}
		}
		logger.Info("resolving spotify")
		src = source.NewSpotifySource(clientID, clientSecret)
	} else if isSoundCloud(url) {
		logger.Info("resolving soundcloud")
		src = source.NewSoundCloudSource()
	} else {
		logger.Info("resolving youtube")
		src = source.NewYouTubeSource()
	}

	tracks, err := src.Resolve(url)
	if err != nil {
		logger.WithError(err).Error("resolve failed")
		return ResolveResult{Error: err.Error()}
	}

	logger.WithField("tracks", len(tracks)).Info("resolve done")
	return ResolveResult{Tracks: toDTO(tracks)}
}

// SearchYouTube searches YouTube by name and returns the top results.
func (a *App) SearchYouTube(query string) ResolveResult {
	log.WithField("query", query).Info("searching youtube")
	tracks, err := source.Search(query, 8)
	if err != nil {
		log.WithError(err).Error("youtube search failed")
		return ResolveResult{Error: err.Error()}
	}
	return ResolveResult{Tracks: toDTO(tracks)}
}

// ── Download ──────────────────────────────────────────────────────────────────

// Download starts downloading all tracks into outputDir.
// Progress is streamed back to the frontend via Wails events:
//   - "progress" { track, status, message }
//   - "done"
func (a *App) Download(tracks []TrackDTO, outputDir string) {
	if err := os.MkdirAll(outputDir, 0755); err != nil {
		log.WithError(err).Error("cannot create output directory")
		runtime.EventsEmit(a.ctx, "done", map[string]string{
			"error": "cannot create directory: " + err.Error(),
		})
		return
	}

	sourceTracks := make([]source.Track, len(tracks))
	for i, d := range tracks {
		sourceTracks[i] = source.Track{Title: d.Title, Artist: d.Artist, URL: d.URL}
	}

	go func() {
		logger := log.WithFields(log.Fields{
			"total":     len(sourceTracks),
			"outputDir": outputDir,
		})
		logger.Info("download started")

		var ok, failed int
		downloader.Run(sourceTracks, outputDir, func(result downloader.Result) {
			name := result.Track.DisplayName()
			if result.Err != nil {
				failed++
				log.WithError(result.Err).WithField("track", name).Error("download failed")
				runtime.EventsEmit(a.ctx, "progress", map[string]string{
					"track":   name,
					"status":  "fail",
					"message": result.Err.Error(),
				})
			} else {
				ok++
				log.WithField("track", name).Info("download ok")
				runtime.EventsEmit(a.ctx, "progress", map[string]string{
					"track":  name,
					"status": "ok",
				})
			}
		})

		log.WithFields(log.Fields{"ok": ok, "failed": failed}).Info("download done")
		runtime.EventsEmit(a.ctx, "done", nil)
	}()
}

// ── Helpers ───────────────────────────────────────────────────────────────────

func isSpotify(url string) bool {
	return strings.Contains(url, "spotify.com") || strings.HasPrefix(url, "spotify:")
}

func isSoundCloud(url string) bool {
	return strings.Contains(url, "soundcloud.com")
}

func toDTO(tracks []source.Track) []TrackDTO {
	dtos := make([]TrackDTO, len(tracks))
	for i, t := range tracks {
		dtos[i] = TrackDTO{
			Title:       t.Title,
			Artist:      t.Artist,
			URL:         t.URL,
			DurationSec: int(t.Duration.Seconds()),
		}
	}
	return dtos
}
