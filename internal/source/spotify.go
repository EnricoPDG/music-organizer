package source

import (
	"context"
	"fmt"
	"strings"

	"github.com/zmb3/spotify/v2"
	spotifyauth "github.com/zmb3/spotify/v2/auth"
	"golang.org/x/oauth2/clientcredentials"
)

// SpotifySource resolves Spotify playlist, album, or track URLs into Tracks.
// Uses Client Credentials flow — only Client ID and Secret required, no user login.
type SpotifySource struct {
	ClientID     string
	ClientSecret string
}

func NewSpotifySource(clientID, clientSecret string) *SpotifySource {
	return &SpotifySource{ClientID: clientID, ClientSecret: clientSecret}
}

func (s *SpotifySource) Resolve(url string) ([]Track, error) {
	ctx := context.Background()

	config := &clientcredentials.Config{
		ClientID:     s.ClientID,
		ClientSecret: s.ClientSecret,
		TokenURL:     spotifyauth.TokenURL,
	}
	client := spotify.New(config.Client(ctx))

	id, kind, err := parseSpotifyURL(url)
	if err != nil {
		return nil, err
	}

	switch kind {
	case "track":
		t, err := client.GetTrack(ctx, spotify.ID(id))
		if err != nil {
			return nil, err
		}
		return []Track{{Title: t.Name, Artist: t.Artists[0].Name, Duration: t.TimeDuration()}}, nil
	case "album":
		return resolveAlbum(ctx, client, spotify.ID(id))
	case "playlist":
		return resolvePlaylist(ctx, client, spotify.ID(id))
	}
	return nil, fmt.Errorf("unsupported spotify resource type: %s", kind)
}

// parseSpotifyURL extracts the resource ID and type from a Spotify URL or URI.
// Handles: https://open.spotify.com/playlist/<id>?... and spotify:<type>:<id>
func parseSpotifyURL(url string) (id, kind string, err error) {
	if strings.Contains(url, "open.spotify.com/") {
		parts := strings.Split(url, "/")
		for i, p := range parts {
			if p == "track" || p == "album" || p == "playlist" {
				if i+1 < len(parts) {
					rawID := parts[i+1]
					if idx := strings.Index(rawID, "?"); idx != -1 {
						rawID = rawID[:idx]
					}
					return rawID, p, nil
				}
			}
		}
	}
	if strings.HasPrefix(url, "spotify:") {
		parts := strings.Split(url, ":")
		if len(parts) == 3 {
			return parts[2], parts[1], nil
		}
	}
	return "", "", fmt.Errorf("cannot parse spotify URL: %s", url)
}

func resolveAlbum(ctx context.Context, client *spotify.Client, id spotify.ID) ([]Track, error) {
	var tracks []Track
	page, err := client.GetAlbumTracks(ctx, id, spotify.Limit(50))
	if err != nil {
		return nil, err
	}
	for {
		for _, t := range page.Tracks {
			if len(t.Artists) == 0 {
				continue
			}
			tracks = append(tracks, Track{Title: t.Name, Artist: t.Artists[0].Name, Duration: t.TimeDuration()})
		}
		if page.Next == "" {
			break
		}
		if err := client.NextPage(ctx, page); err != nil {
			return nil, err
		}
	}
	return tracks, nil
}

func resolvePlaylist(ctx context.Context, client *spotify.Client, id spotify.ID) ([]Track, error) {
	var tracks []Track
	page, err := client.GetPlaylistTracks(ctx, id, spotify.Limit(100))
	if err != nil {
		return nil, err
	}
	for {
		for _, item := range page.Tracks {
			t := item.Track
			if t.ID == "" || len(t.Artists) == 0 {
				continue // skip episodes / local files / empty items
			}
			tracks = append(tracks, Track{Title: t.Name, Artist: t.Artists[0].Name, Duration: t.TimeDuration()})
		}
		if page.Next == "" {
			break
		}
		if err := client.NextPage(ctx, page); err != nil {
			return nil, err
		}
	}
	return tracks, nil
}
