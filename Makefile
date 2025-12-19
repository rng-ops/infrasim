# InfraSim Makefile
.PHONY: all build clean test install uninstall dev release docker help ui-install ui-dev ui-build ui-typecheck ui-lint dist isvm-install isvm-link isvm-use

# Configuration
VERSION ?= $(shell git describe --tags --always --dirty 2>/dev/null || echo "dev")
PREFIX ?= /usr/local
CARGO_FLAGS ?=
PROFILE ?= release
ISVM_DIR ?= $(HOME)/.isvm

help: ## Show this help message
	@echo "InfraSim Build System"
	@echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2}'

all: build ## Build all binaries (default)

ui-install: ## Install UI dependencies (pnpm)
	cd ui && pnpm install

ui-dev: ## Run Console UI dev server (Vite)
	cd ui && pnpm -C apps/console dev

ui-build: ## Build Console UI for production
	cd ui && pnpm -C apps/console build

ui-typecheck: ## Typecheck UI workspace
	cd ui && pnpm -r typecheck

ui-lint: ## Lint UI workspace
	cd ui && pnpm -r lint

build: ## Build release binaries
	@echo "🏗️  Building InfraSim $(VERSION)..."
	cargo build --profile $(PROFILE) --all
	@echo "✅ Build complete!"

dev: ## Build debug binaries
	@$(MAKE) build PROFILE=dev

release: clean ## Build production release with full pipeline
	@chmod +x build.sh
	./build.sh

test: ## Run all tests
	@echo "🧪 Running tests..."
	cargo test --all --verbose

clean: ## Clean build artifacts
	@echo "🧹 Cleaning..."
	cargo clean
	rm -rf dist

install: build ## Install binaries to system
	@echo "📦 Installing to $(PREFIX)/bin..."
	install -m 755 target/$(PROFILE)/infrasim $(PREFIX)/bin/
	install -m 755 target/$(PROFILE)/infrasimd $(PREFIX)/bin/
	@mkdir -p ~/.terraform.d/plugins/registry.terraform.io/infrasim/infrasim/$(VERSION)/darwin_arm64
	install -m 755 target/$(PROFILE)/terraform-provider-infrasim \
		~/.terraform.d/plugins/registry.terraform.io/infrasim/infrasim/$(VERSION)/darwin_arm64/
	@echo "✅ Installation complete!"
	@echo "Run 'infrasim --help' to get started"

uninstall: ## Remove installed binaries
	@echo "🗑️  Uninstalling..."
	rm -f $(PREFIX)/bin/infrasim
	rm -f $(PREFIX)/bin/infrasimd
	rm -rf ~/.terraform.d/plugins/registry.terraform.io/infrasim
	@echo "✅ Uninstall complete"

check: ## Run cargo check
	cargo check --all

fmt: ## Format code
	cargo fmt --all

lint: ## Run clippy
	cargo clippy --all -- -D warnings

docs: ## Generate documentation
	cargo doc --all --no-deps --open

run-daemon: build ## Run daemon in foreground
	./target/$(PROFILE)/infrasimd --config config.toml

run-cli: build ## Run CLI (interactive)
	./target/$(PROFILE)/infrasim status

benchmark: ## Run benchmarks
	cargo bench --all

coverage: ## Generate code coverage report
	@echo "📊 Generating coverage report..."
	cargo tarpaulin --out Html --output-dir coverage
	@echo "✅ Coverage report: coverage/index.html"

# Development helpers
watch: ## Watch and rebuild on changes
	cargo watch -x 'build --all'

proto: ## Regenerate proto files
	@echo "🔧 Regenerating protobuf files..."
	cargo clean -p infrasim-common -p infrasim-daemon -p infrasim-provider -p infrasim-cli
	cargo build

# Docker targets
docker: ## Build Docker image
	docker build -t infrasim/daemon:$(VERSION) .

docker-run: docker ## Run daemon in Docker
	docker run -it --rm \
		-v /var/run/qemu:/var/run/qemu \
		-p 50051:50051 \
		infrasim/daemon:$(VERSION)

# Package for distribution
package: release ## Create distribution packages
	@echo "📦 Creating distribution packages..."
	@mkdir -p dist/packages
	cd dist && tar -czf packages/infrasim-$(VERSION)-macos-arm64.tar.gz \
		infrasim infrasimd terraform-provider-infrasim manifest.json
	@echo "✅ Package created: dist/packages/infrasim-$(VERSION)-macos-arm64.tar.gz"

# Quick smoke test
smoke: build ## Run smoke tests
	@echo "🚬 Running smoke tests..."
	./target/$(PROFILE)/infrasim --version
	./target/$(PROFILE)/infrasimd --help
	./target/$(PROFILE)/terraform-provider-infrasim --help || true
	@echo "✅ Smoke tests passed"

# Size analysis
size: build ## Show binary sizes
	@echo "📊 Binary sizes:"
	@ls -lh target/$(PROFILE)/{infrasim,infrasimd,terraform-provider-infrasim} | awk '{print "  " $$9 " → " $$5}'

# Dependency check
deps: ## Check for outdated dependencies
	cargo outdated

audit: ## Security audit
	cargo audit

# Full CI pipeline
ci: clean check lint test build smoke ## Run full CI pipeline
	@echo "✅ CI pipeline complete!"

dist: ui-build build ## Build UI + Rust binaries
	@echo "✅ Dist complete (UI + Rust)"

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# ISVM - InfraSim Version Manager
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

isvm-setup: ## Install ISVM (InfraSim Version Manager)
	@chmod +x scripts/install-isvm.sh
	@./scripts/install-isvm.sh

isvm-install: build ## Build and install current version via ISVM
	@if [ -f "$(ISVM_DIR)/isvm.sh" ]; then \
		source "$(ISVM_DIR)/isvm.sh" && isvm install "$(VERSION)"; \
	else \
		echo "ISVM not installed. Run 'make isvm-setup' first"; \
		exit 1; \
	fi

isvm-link: build ## Link project binaries to PATH (dev mode)
	@if [ -f "$(ISVM_DIR)/isvm.sh" ]; then \
		source "$(ISVM_DIR)/isvm.sh" && isvm link; \
	else \
		echo "ISVM not installed. Run 'make isvm-setup' first"; \
		exit 1; \
	fi

isvm-use: ## Switch to a version (usage: make isvm-use V=v0.1.0)
	@if [ -z "$(V)" ]; then \
		echo "Usage: make isvm-use V=<version>"; \
		echo "Example: make isvm-use V=v0.1.0"; \
		exit 1; \
	fi
	@if [ -f "$(ISVM_DIR)/isvm.sh" ]; then \
		source "$(ISVM_DIR)/isvm.sh" && isvm use "$(V)"; \
	else \
		echo "ISVM not installed. Run 'make isvm-setup' first"; \
		exit 1; \
	fi

isvm-list: ## List installed ISVM versions
	@if [ -f "$(ISVM_DIR)/isvm.sh" ]; then \
		source "$(ISVM_DIR)/isvm.sh" && isvm list; \
	else \
		echo "ISVM not installed. Run 'make isvm-setup' first"; \
	fi
