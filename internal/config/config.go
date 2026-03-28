package config

import (
	"encoding/json"
	"os"
	"path/filepath"
)

type Config struct {
	OutputDir string `json:"outputDir"`
}

func dir() (string, error) {
	base, err := os.UserConfigDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(base, "music-organizer"), nil
}

func Load() Config {
	d, err := dir()
	if err != nil {
		return Config{}
	}
	data, err := os.ReadFile(filepath.Join(d, "config.json"))
	if err != nil {
		return Config{}
	}
	var c Config
	json.Unmarshal(data, &c)
	return c
}

func Save(c Config) error {
	d, err := dir()
	if err != nil {
		return err
	}
	if err := os.MkdirAll(d, 0700); err != nil {
		return err
	}
	data, err := json.Marshal(c)
	if err != nil {
		return err
	}
	return os.WriteFile(filepath.Join(d, "config.json"), data, 0600)
}
