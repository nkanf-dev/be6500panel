.PHONY: setup dev-api dev-web test build armv7 clean

setup:
	cd web && bun install --frozen-lockfile

dev-api:
	go run ./cmd/be6500panel --listen 127.0.0.1:8787 --demo

dev-web:
	cd web && bun run dev

test:
	go test ./...
	go vet ./...
	cd web && bun run typecheck && bun run test

build:
	mkdir -p dist
	go build -trimpath -ldflags='-s -w' -o dist/be6500panel ./cmd/be6500panel
	cd web && bun run build

armv7:
	mkdir -p dist
	CGO_ENABLED=0 GOOS=linux GOARCH=arm GOARM=7 go build -trimpath -ldflags='-s -w' -o dist/be6500panel-linux-armv7 ./cmd/be6500panel

clean:
	rm -rf dist web/dist
