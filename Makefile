.PHONY: setup dev-api dev-web test test-rust test-web build armv7 clean
.NOTPARALLEL:

CARGO ?= cargo
CARGO_MANIFEST := rust/panel/Cargo.toml
RUST_TARGET_DIR ?= $(CURDIR)/.build/rust
export CARGO_TARGET_DIR := $(RUST_TARGET_DIR)
export CARGO_BUILD_JOBS := 1
export CARGO_INCREMENTAL := 0
# Process/fault fixtures share native ownership slots; run test cases serially.
export RUST_TEST_THREADS := 1

# Cross-native dependencies need an ARM-capable archiver as well as a linker.
# Override these for macOS LLVM cross-builds; never use Apple ar on ELF objects.
ARM_CC ?= arm-linux-gnueabihf-gcc
ARM_AR ?= arm-linux-gnueabihf-ar
ARM_LINKER ?= $(ARM_CC)
ARM_CFLAGS ?=
ARM_RUSTFLAGS ?= $(RUSTFLAGS)

setup:
	cd web && bun install --frozen-lockfile

dev-api:
	$(CARGO) run --locked --manifest-path $(CARGO_MANIFEST) -- --listen 127.0.0.1:8790

dev-web:
	cd web && bun run dev

test: test-rust test-web

test-rust:
	$(CARGO) test --locked --manifest-path $(CARGO_MANIFEST) --all-targets
	$(CARGO) fmt --manifest-path $(CARGO_MANIFEST) --check
	$(CARGO) clippy --locked --manifest-path $(CARGO_MANIFEST) --all-targets -- -D warnings

test-web:
	cd web && bun run typecheck && bun run test -- --maxWorkers=1

build:
	$(CARGO) build --locked --release --manifest-path $(CARGO_MANIFEST)
	cd web && bun run build

armv7:
	env CC_armv7_unknown_linux_musleabihf="$(ARM_CC)" AR_armv7_unknown_linux_musleabihf="$(ARM_AR)" CFLAGS_armv7_unknown_linux_musleabihf="$(ARM_CFLAGS)" CARGO_TARGET_ARMV7_UNKNOWN_LINUX_MUSLEABIHF_LINKER="$(ARM_LINKER)" RUSTFLAGS="$(ARM_RUSTFLAGS)" $(CARGO) build --locked --release --manifest-path $(CARGO_MANIFEST) --target armv7-unknown-linux-musleabihf

clean:
	rm -rf .build dist web/dist
