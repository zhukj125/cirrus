.DEFAULT_GOAL := dev

PNPM ?= pnpm
TAURI_DEV_PORT ?= 1420

.PHONY: help install dev dev-web build package check test cargo-check cargo-test cargo-fmt cargo-clippy fmt lint clean

help:
	@printf '%s\n' 'Cirrus development targets:'
	@printf '%s\n' ''
	@printf '%s\n' 'App:'
	@printf '  %-22s %s\n' 'make' 'Start the desktop dev environment (Vite + Tauri window)'
	@printf '  %-22s %s\n' 'make dev' 'Alias for make dev'
	@printf '  %-22s %s\n' 'make dev-web' 'Start the frontend dev server only'
	@printf '  %-22s %s\n' 'make build' 'Type-check and build the frontend bundle'
	@printf '  %-22s %s\n' 'make package' 'Package the desktop app (NSIS installer)'
	@printf '%s\n' ''
	@printf '%s\n' 'Checks:'
	@printf '  %-22s %s\n' 'make check' 'Format, lint, type-check, and run frontend tests'
	@printf '  %-22s %s\n' 'make test' 'Run frontend tests only'
	@printf '  %-22s %s\n' 'make cargo-check' 'Run cargo check for the workspace'
	@printf '  %-22s %s\n' 'make cargo-test' 'Run cargo tests for the workspace'
	@printf '  %-22s %s\n' 'make cargo-clippy' 'Run clippy with warnings denied'
	@printf '  %-22s %s\n' 'make fmt' 'Format Rust and frontend sources'
	@printf '%s\n' ''
	@printf '%s\n' 'Setup:'
	@printf '  %-22s %s\n' 'make install' 'Install frontend dependencies'
	@printf '  %-22s %s\n' 'make clean' 'Remove Rust build artifacts'

node_modules/.modules.yaml: package.json pnpm-lock.yaml
	$(PNPM) install --frozen-lockfile

install:
	$(PNPM) install

ifeq ($(OS),Windows_NT)
check-tauri-dev-port:
	@powershell -NoProfile -Command "if (Get-NetTCPConnection -LocalPort $(TAURI_DEV_PORT) -State Listen -ErrorAction SilentlyContinue) { Write-Host 'Port $(TAURI_DEV_PORT) is already in use. Tauri dev requires http://localhost:$(TAURI_DEV_PORT).'; Write-Host ''; Get-NetTCPConnection -LocalPort $(TAURI_DEV_PORT) -State Listen -ErrorAction SilentlyContinue | Format-Table LocalAddress,LocalPort,OwningProcess -AutoSize; Write-Host 'Stop the process above, then run make dev again.'; exit 1 }"
else
check-tauri-dev-port:
	@if lsof -nP -iTCP:$(TAURI_DEV_PORT) -sTCP:LISTEN >/dev/null 2>&1; then \
		echo "Port $(TAURI_DEV_PORT) is already in use. Tauri dev requires http://localhost:$(TAURI_DEV_PORT)."; \
		echo ""; \
		lsof -nP -iTCP:$(TAURI_DEV_PORT) -sTCP:LISTEN; \
		echo ""; \
		echo "Stop the process above, then run make dev again."; \
		exit 1; \
	fi
endif

dev: node_modules/.modules.yaml check-tauri-dev-port
	$(PNPM) dev:tauri

dev-web: node_modules/.modules.yaml
	$(PNPM) dev

build: node_modules/.modules.yaml
	$(PNPM) build:checked

package: node_modules/.modules.yaml
	$(PNPM) tauri build

clean:
	cargo clean

check: node_modules/.modules.yaml
	$(PNPM) check

test: node_modules/.modules.yaml
	$(PNPM) test

fmt:
	cargo fmt --all
	$(PNPM) fmt

lint: node_modules/.modules.yaml
	$(PNPM) lint

cargo-check:
	cargo check --workspace --all-targets

cargo-test:
	cargo test --workspace

cargo-clippy:
	cargo clippy --workspace --all-targets -- -D warnings
