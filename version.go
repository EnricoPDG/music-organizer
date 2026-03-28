package main

// Version is the application version.
// In production builds it is injected at link time via:
//
//	wails build -ldflags "-X main.Version=x.y.z"
//
// During local development it defaults to "dev".
var Version = "dev"
