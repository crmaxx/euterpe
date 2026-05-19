# Euterpe development shortcuts
.PHONY: help prepare backend frontend frontend-install frontend-generate frontend-dev dev dev-stop
.PHONY: test test-backend test-frontend
.PHONY: help-release rustup-targets
.PHONY: release-linux-amd64 release-windows-amd64 release-arm-pi1 release-all
.PHONY: cross-release-linux-amd64 cross-release-windows-amd64 cross-release-arm-pi1 cross-release-all
.PHONY: dist dist-linux-amd64 dist-windows-amd64 dist-arm-pi1 dist-all

FRONTEND_DIR := frontend
PKG := euterpe-server
# IDE / non-login shells often omit HOME; fall back to passwd home (macOS id -P, Linux getent).
USER_HOME := $(if $(HOME),$(HOME),$(shell \
	/usr/bin/id -P 2>/dev/null | /usr/bin/awk -F: '{print $$9; exit}' || \
	getent passwd $$(id -un 2>/dev/null) 2>/dev/null | cut -d: -f6))
# Non-interactive make does not load shell rc; put common Rust install paths first.
export PATH := $(USER_HOME)/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$(PATH)
# Prefer absolute path so recipes work when PATH export is ignored or HOME was wrong.
CARGO := $(firstword \
	$(wildcard $(USER_HOME)/.cargo/bin/cargo) \
	$(shell PATH="$(USER_HOME)/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$$PATH" command -v cargo 2>/dev/null))
ifeq ($(CARGO),)
  CARGO := cargo
endif
CROSS ?= cross
DIST := dist

TARGET_LINUX_AMD64 := x86_64-unknown-linux-gnu
TARGET_WINDOWS_AMD64 := x86_64-pc-windows-gnu
TARGET_ARM_PI1 := arm-unknown-linux-gnueabihf

RELEASE_LINUX := target/$(TARGET_LINUX_AMD64)/release/$(PKG)
RELEASE_WINDOWS := target/$(TARGET_WINDOWS_AMD64)/release/$(PKG).exe
RELEASE_ARM_PI1 := target/$(TARGET_ARM_PI1)/release/$(PKG)

help:
	@echo "Targets:"
	@echo "  make prepare              Dev tools: overmind, cross, rustup targets, npm ci, husky"
	@echo "  make backend              Run API server (cargo run -p euterpe-server)"
	@echo "  make frontend-install     cd frontend && npm ci"
	@echo "  make frontend-generate    cd frontend && npm run generate:api"
	@echo "  make frontend-dev         cd frontend && npm run dev"
	@echo "  make frontend             install + generate + dev (Vite on :5173)"
	@echo "  make dev                  overmind start (Procfile: backend + frontend)"
	@echo "  make dev-stop             overmind quit"
	@echo "  make test                 Run backend + frontend tests"
	@echo "  make test-backend         cargo test --workspace"
	@echo "  make test-frontend        frontend: generate:api + npm test"
	@echo ""
	@echo "Release / cross-compile (see: make help-release)"

help-release:
	@echo "Rust targets: $(TARGET_LINUX_AMD64), $(TARGET_WINDOWS_AMD64), $(TARGET_ARM_PI1)"
	@echo "  make rustup-targets           Install targets via rustup"
	@echo "  make release-linux-amd64      cargo build (native on Linux amd64)"
	@echo "  make release-windows-amd64    cargo build (needs mingw linker on host)"
	@echo "  make release-arm-pi1          cargo build (needs arm-linux-gnueabihf-gcc)"
	@echo "  make release-all              All three via cargo"
	@echo "  make cross-release-all        All three via cross-rs (Docker, recommended on macOS)"
	@echo "  make dist-all                 Copy binaries to $(DIST)/"
	@echo "Docs: docs/04-deployment/cross-compile.ru.md"

rustup-targets:
	@rustup_bin=$$(command -v rustup); \
	if [ -z "$$rustup_bin" ]; then \
		echo "rustup not found in PATH ($$PATH)"; \
		echo "Install: https://rustup.rs  (or: brew install rustup-init && rustup-init)"; \
		echo "Homebrew 'cargo' alone does not provide rustup target add."; \
		exit 1; \
	fi; \
	"$$rustup_bin" target add $(TARGET_LINUX_AMD64) $(TARGET_WINDOWS_AMD64) $(TARGET_ARM_PI1)

release-linux-amd64:
	$(CARGO) build --release -p $(PKG) --target $(TARGET_LINUX_AMD64)

release-windows-amd64:
	$(CARGO) build --release -p $(PKG) --target $(TARGET_WINDOWS_AMD64)

release-arm-pi1:
	$(CARGO) build --release -p $(PKG) --target $(TARGET_ARM_PI1)

release-all: release-linux-amd64 release-windows-amd64 release-arm-pi1

cross-release-linux-amd64:
	$(CROSS) build --release -p $(PKG) --target $(TARGET_LINUX_AMD64)

cross-release-windows-amd64:
	$(CROSS) build --release -p $(PKG) --target $(TARGET_WINDOWS_AMD64)

# ARMv6 (Raspberry Pi 1 / DietPi B+): must not use ARMv7-only instructions.
PI1_RUSTFLAGS := -C target-cpu=arm1176jzf-s

cross-release-arm-pi1:
	CARGO_TARGET_ARM_UNKNOWN_LINUX_GNUEABIHF_RUSTFLAGS="$(PI1_RUSTFLAGS)" \
		$(CROSS) build --release -p $(PKG) --target $(TARGET_ARM_PI1)

cross-release-all: cross-release-linux-amd64 cross-release-windows-amd64 cross-release-arm-pi1

dist-linux-amd64: release-linux-amd64
	mkdir -p $(DIST)/linux-amd64
	cp $(RELEASE_LINUX) $(DIST)/linux-amd64/$(PKG)

dist-windows-amd64: release-windows-amd64
	mkdir -p $(DIST)/windows-amd64
	cp $(RELEASE_WINDOWS) $(DIST)/windows-amd64/$(PKG).exe

dist-arm-pi1: release-arm-pi1
	mkdir -p $(DIST)/arm-pi1
	cp $(RELEASE_ARM_PI1) $(DIST)/arm-pi1/$(PKG)

dist-all: dist-linux-amd64 dist-windows-amd64 dist-arm-pi1

# Copy after cross-release-all (same paths under target/<triple>/release/)
dist-cross: cross-release-all
	mkdir -p $(DIST)/linux-amd64 $(DIST)/windows-amd64 $(DIST)/arm-pi1
	cp $(RELEASE_LINUX) $(DIST)/linux-amd64/$(PKG)
	cp $(RELEASE_WINDOWS) $(DIST)/windows-amd64/$(PKG).exe
	cp $(RELEASE_ARM_PI1) $(DIST)/arm-pi1/$(PKG)

prepare:
	@command -v overmind >/dev/null 2>&1 || brew install overmind
	@command -v $(CROSS) >/dev/null 2>&1 || \
		$(CARGO) install cross --git https://github.com/cross-rs/cross
	$(MAKE) rustup-targets
	npm ci
	cd $(FRONTEND_DIR) && npm ci

backend:
	@test -x "$(CARGO)" || command -v "$(CARGO)" >/dev/null 2>&1 || { echo "cargo not found — https://rustup.rs"; exit 1; }
	$(CARGO) run -p euterpe-server

frontend-install:
	cd $(FRONTEND_DIR) && npm ci

frontend-generate: frontend-install
	cd $(FRONTEND_DIR) && npm run generate:api

frontend-dev: frontend-generate
	cd $(FRONTEND_DIR) && npm run dev

frontend: frontend-dev

dev:
	overmind start

dev-stop:
	overmind quit

test-backend:
	@if [ ! -x "$(CARGO)" ] && ! command -v "$(CARGO)" >/dev/null 2>&1; then \
		echo "cargo not found (looked for: $(CARGO), PATH=$(PATH))"; \
		echo "Install Rust: https://rustup.rs"; \
		exit 1; \
	fi
	$(CARGO) test --workspace

test-frontend: frontend-generate
	cd $(FRONTEND_DIR) && npm test

test: test-backend test-frontend
