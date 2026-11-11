package main

import (
	"embed"
	"fmt"
	"io/fs"
	"os"

	"github.com/wailsapp/wails/v2"
	"github.com/wailsapp/wails/v2/pkg/options"
	"github.com/wailsapp/wails/v2/pkg/options/assetserver"
)

//go:embed all:frontend/dist
var assets embed.FS

// stressFS serves stress.html as the index, so Wails injects its runtime (window.go) into it.
type stressFS struct{ fs.FS }

func (s stressFS) Open(name string) (fs.File, error) {
	if name == "frontend/dist/index.html" {
		name = "frontend/dist/stress.html"
	}
	return s.FS.Open(name)
}

// App is bound to the page as window.go.main.App.
type App struct{}

// Report takes the stress page's `mark` / `done` lines. ../stress.py reads stdout and stops the app.
func (a *App) Report(line string) {
	fmt.Println(line)
}

func main() {
	stress := os.Getenv("STRESS") != ""
	w, h := 480, 520
	var files fs.FS = assets
	if stress {
		w, h = 1280, 800
		files = stressFS{assets}
	}
	err := wails.Run(&options.App{
		Title:       "Todos",
		Width:       w,
		Height:      h,
		AssetServer: &assetserver.Options{Assets: files},
		Bind:        []interface{}{&App{}},
	})
	if err != nil {
		println("Error:", err.Error())
	}
}
