package main

import (
	"embed"

	"github.com/wailsapp/wails/v2"
	"github.com/wailsapp/wails/v2/pkg/options"
	"github.com/wailsapp/wails/v2/pkg/options/assetserver"
)

//go:embed all:frontend/dist
var assets embed.FS

func main() {
	err := wails.Run(&options.App{
		Title:       "Todos",
		Width:       480,
		Height:      520,
		AssetServer: &assetserver.Options{Assets: assets},
	})
	if err != nil {
		println("Error:", err.Error())
	}
}
