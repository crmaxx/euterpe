# Euterpe development shortcuts
.PHONY: help prepare backend frontend frontend-install frontend-generate frontend-dev dev dev-stop dev-local
.PHONY: test test-backend test-frontend

FRONTEND_DIR := frontend
PKG := euterpe-server
DEV_COMPOSE := docker compose -f docker/compose.dev.yml --project-name euterpe-dev
export NPM_CONFIG_CACHE := $(CURDIR)/.npm-cache
export NPM_CONFIG_UPDATE_NOTIFIER := false
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
MISE := $(shell PATH="$(USER_HOME)/.local/bin:/opt/homebrew/bin:/usr/local/bin:$$PATH" command -v mise 2>/dev/null)
ifneq ($(MISE),)
  NPM := $(MISE) exec -- npm
else
  NPM := npm
endif

help:
	@echo "Targets:"
	@echo "  make prepare              Dev tools: overmind, npm ci, husky"
	@echo "  make backend              Run API server (cargo run -p euterpe-server)"
	@echo "  make frontend-install     cd frontend && npm ci"
	@echo "  make frontend-generate    cd frontend && npm run generate:api"
	@echo "  make frontend-dev         cd frontend && npm run dev"
	@echo "  make frontend             install + generate + dev (Vite on :5173)"
	@echo "  make dev                  Docker Compose dev stack (UI :5173, API :9080)"
	@echo "  make dev-stop             Stop Docker Compose dev stack"
	@echo "  make dev-local            overmind start (Procfile: backend + frontend)"
	@echo "  make test                 Run backend + frontend tests"
	@echo "  make test-backend         cargo test --workspace"
	@echo "  make test-frontend        frontend: generate:api + npm test"

prepare:
	@command -v overmind >/dev/null 2>&1 || brew install overmind
	$(NPM) ci
	cd $(FRONTEND_DIR) && $(NPM) ci

backend:
	@test -x "$(CARGO)" || command -v "$(CARGO)" >/dev/null 2>&1 || { echo "cargo not found — https://rustup.rs"; exit 1; }
	$(CARGO) run -p euterpe-server --release

frontend-install:
	cd $(FRONTEND_DIR) && $(NPM) ci

frontend-generate: frontend-install
	cd $(FRONTEND_DIR) && $(NPM) run generate:api

frontend-dev: frontend-generate
	cd $(FRONTEND_DIR) && $(NPM) run dev

frontend: frontend-dev

dev:
	$(DEV_COMPOSE) up --build

dev-stop:
	$(DEV_COMPOSE) down

dev-local:
	overmind start

test-backend:
	@if [ ! -x "$(CARGO)" ] && ! command -v "$(CARGO)" >/dev/null 2>&1; then \
		echo "cargo not found (looked for: $(CARGO), PATH=$(PATH))"; \
		echo "Install Rust: https://rustup.rs"; \
		exit 1; \
	fi
	$(CARGO) test --workspace

test-frontend: frontend-generate
	cd $(FRONTEND_DIR) && $(NPM) test

test: test-backend test-frontend
