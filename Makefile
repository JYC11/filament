.PHONY: fmt
fmt: ## Format crates (usage: make fmt CRATE=filament-core, make fmt CRATE=all CHECK=1)
	@if [ -z "$(CRATE)" ]; then \
		echo "Usage: make fmt CRATE=<crate-name|all> [CHECK=1]"; \
		echo ""; \
		echo "Available crates: filament-core, filament-cli, filament-daemon, filament-tui, all"; \
		exit 1; \
	fi
	@bash util-scripts/fmt.sh $(CRATE) $(if $(CHECK),--check)

.PHONY: check
check: ## Type-check a crate with optional clippy (usage: make check CRATE=filament-core, make check CRATE=all CLIPPY=1)
	@if [ -z "$(CRATE)" ]; then \
		echo "Usage: make check CRATE=<crate-name|all> [CLIPPY=1]"; \
		echo ""; \
		echo "Available crates: filament-core, filament-cli, filament-daemon, filament-tui, all"; \
		exit 1; \
	fi
	@bash util-scripts/check.sh $(CRATE) $(if $(CLIPPY),--clippy)

.PHONY: build
build: ## Build a crate (usage: make build CRATE=filament-core, make build CRATE=all RELEASE=1)
	@if [ -z "$(CRATE)" ]; then \
		echo "Usage: make build CRATE=<crate-name|all> [RELEASE=1]"; \
		echo ""; \
		echo "Available crates: filament-core, filament-cli, filament-daemon, filament-tui, all"; \
		exit 1; \
	fi
	@bash util-scripts/build.sh $(CRATE) $(if $(RELEASE),--release)

.PHONY: test
test: ## Run tests (usage: make test CRATE=filament-core or make test CRATE=all)
	@if [ -z "$(CRATE)" ]; then \
		echo "Usage: make test CRATE=<crate-name|all>"; \
		echo ""; \
		echo "Available crates: filament-core, filament-cli, filament-daemon, filament-tui, all"; \
		exit 1; \
	fi
	@bash util-scripts/test.sh $(CRATE)

.PHONY: migration
migration: ## Create a new migration file (usage: make migration NAME=init)
	@if [ -z "$(NAME)" ]; then \
		bash util-scripts/migration.sh; \
	else \
		bash util-scripts/migration.sh $(NAME); \
	fi

.PHONY: run
run: ## Run a binary (usage: make run BIN=filament-cli ARGS="init")
	@if [ -z "$(BIN)" ]; then \
		echo "Usage: make run BIN=<binary-name> [ARGS=\"...\"]"; \
		echo ""; \
		echo "Available binaries: filament-cli, filament-daemon, filament-tui"; \
		exit 1; \
	fi
	@bash util-scripts/run.sh $(BIN) $(if $(ARGS),-- $(ARGS))

.PHONY: adr
adr: ## Create a new ADR (usage: make adr or make adr TITLE="use sqlite for storage")
	@bash util-scripts/adr.sh $(TITLE)

.PHONY: ci
ci: ## Run full CI pipeline: fmt check, clippy, tests
	@echo "=== Format check ==="
	@bash util-scripts/fmt.sh all --check
	@echo ""
	@echo "=== Clippy ==="
	@bash util-scripts/check.sh all --clippy
	@echo ""
	@echo "=== Tests ==="
	@bash util-scripts/test.sh all

.PHONY: install
install: ## Build and install filament to ~/.local/bin (usage: make install [DEST=/usr/local/bin])
	@bash util-scripts/install.sh $(DEST)

.PHONY: uninstall
uninstall: ## Remove filament from ~/.local/bin (usage: make uninstall [DEST=/usr/local/bin])
	@bash util-scripts/uninstall.sh $(DEST)

.PHONY: help
help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-15s\033[0m %s\n", $$1, $$2}'
