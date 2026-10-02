package httpapi

import (
	"net/http"
	"os"
	"path"
	"path/filepath"
	"strings"
)

func staticHandler(directory string) http.Handler {
	if directory == "" {
		directory = "web/dist"
	}
	root, err := filepath.Abs(directory)
	if err != nil {
		return missingUI()
	}
	files := http.FileServer(http.Dir(root))
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != "GET" && r.Method != "HEAD" {
			w.Header().Set("Allow", "GET, HEAD")
			http.Error(w, "Method not allowed", 405)
			return
		}
		if _, err := os.Stat(filepath.Join(root, "index.html")); err != nil {
			missingUI().ServeHTTP(w, r)
			return
		}
		clean := path.Clean("/" + r.URL.Path)
		target := filepath.Join(root, filepath.FromSlash(strings.TrimPrefix(clean, "/")))
		if info, err := os.Stat(target); err == nil && !info.IsDir() {
			files.ServeHTTP(w, r)
			return
		}
		// Missing assets are never HTML. Extension-free paths are browser routes.
		if strings.Contains(filepath.Base(clean), ".") {
			http.NotFound(w, r)
			return
		}
		w.Header().Set("Cache-Control", "no-cache")
		http.ServeFile(w, r, filepath.Join(root, "index.html"))
	})
}
func missingUI() http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		http.Error(w, "UI assets unavailable. Build the browser app and use --web-dir web/dist. JSON APIs remain available.", 503)
	})
}
