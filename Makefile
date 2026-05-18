# Euterpe development shortcuts
.PHONY: help prepare backend frontend frontend-install frontend-generate frontend-dev dev dev-stop
.PHONY: test test-backend test-frontend

FRONTEND_DIR := frontend

help:
	@echo "Targets:"
	@echo "  make prepare              Dev tools: overmind, npm ci, husky pre-commit (clippy + frontend)"
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

prepare:
	@command -v overmind >/dev/null 2>&1 || brew install overmind
	npm ci
	cd $(FRONTEND_DIR) && npm ci

backend:
	cargo run -p euterpe-server

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
	cargo test --workspace

test-frontend: frontend-generate
	cd $(FRONTEND_DIR) && npm test

test: test-backend test-frontend
