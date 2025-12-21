SHELL := /usr/bin/env bash

CODEX_HOME ?= $(HOME)/.codex
PLUGIN_DIR := codex-visual-loop-plugin
SKILL_SRC := skills/codex-visual-loop
SKILL_DST := $(CODEX_HOME)/skills/codex-visual-loop

RUST_MANIFEST := $(PLUGIN_DIR)/Cargo.toml
RUST_BIN_DEBUG := $(PLUGIN_DIR)/target/debug/codex-visual-loop
RUST_BIN_RELEASE := $(PLUGIN_DIR)/target/release/codex-visual-loop
RUST_ENV := CODEX_VISUAL_LOOP_RUST_BIN
BOOTSTRAP_SCRIPT := $(PLUGIN_DIR)/scripts/bootstrap.sh
DOCTOR_SCRIPT := $(PLUGIN_DIR)/scripts/doctor.sh
HAPPY_PATH_SCRIPT := $(PLUGIN_DIR)/scripts/happy_path.sh

.DEFAULT_GOAL := help

.PHONY: help install bootstrap doctor happy-path codex-auto inbox-feedback explain-app install-plugin install-skill verify typecheck test uninstall uninstall-plugin uninstall-skill

help: ## Show available commands
	@echo "codex-visual-loop-plugin Make targets"
	@echo ""
	@grep -E '^[a-zA-Z_-]+:.*?## ' Makefile | awk 'BEGIN {FS = ":.*?## "}; {printf "  %-18s %s\n", $$1, $$2}'

install: install-plugin install-skill ## Install plugin + Codex skill wrapper

bootstrap: ## One-command setup: build/install/skill/PATH/codex-auto wrapper
	@bash "./$(BOOTSTRAP_SCRIPT)"

doctor: ## Diagnose PATH/PEP668/permissions/runtime readiness
	@bash "./$(DOCTOR_SCRIPT)"

happy-path: ## Run bootstrap+doctor+verify+tests and generate a sample packet
	@APP="$(APP)" AX_DEPTH="$(AX_DEPTH)" bash "./$(HAPPY_PATH_SCRIPT)"

codex-auto: ## Launch Codex with approvals+sandbox bypass (dangerous)
	@codex --dangerously-bypass-approvals-and-sandbox

inbox-feedback: ## Run OMX inbox-aware feedback helper (ARGS='--json --execute')
	@if command -v codex-visual-loop >/dev/null 2>&1; then \
		codex-visual-loop visual-loop-feedback $(ARGS); \
	else \
		python3 -m codex_visual_loop_plugin.visual_loop_feedback $(ARGS); \
	fi

explain-app: ## Build capture+AX packet and generate detailed app explanation report
	@if command -v codex-visual-loop >/dev/null 2>&1; then \
		codex-visual-loop explain-app $(ARGS); \
	else \
		python3 -m codex_visual_loop_plugin.cli explain-app $(ARGS); \
	fi

install-plugin: ## Build Rust backend (when present) and install Python wrapper entrypoints
	@if [[ -f "./$(RUST_MANIFEST)" ]] && command -v cargo >/dev/null 2>&1; then \
		echo "Building Rust backend (release)..."; \
		cargo build --release --manifest-path "./$(RUST_MANIFEST)"; \
	else \
		echo "Rust manifest not found at ./$(RUST_MANIFEST); skipping Rust build"; \
	fi
	@python3 -m pip install -e ./$(PLUGIN_DIR) || \
	python3 -m pip install --user --break-system-packages -e ./$(PLUGIN_DIR)

install-skill: ## Install skill wrapper into $$CODEX_HOME/skills
	@mkdir -p "$(SKILL_DST)"
	@rm -rf "$(SKILL_DST)"
	@mkdir -p "$(SKILL_DST)"
	@cp -R "$(SKILL_SRC)/." "$(SKILL_DST)/"
	@echo "Installed skill to $(SKILL_DST)"

verify: ## Verify CLI command is available and Rust backend can be invoked
	@if [[ -f "./$(RUST_MANIFEST)" ]] && command -v cargo >/dev/null 2>&1; then \
		cargo build --manifest-path "./$(RUST_MANIFEST)" >/dev/null; \
		"$$PWD/$(RUST_BIN_DEBUG)" commands; \
	elif command -v codex-visual-loop >/dev/null 2>&1; then \
		codex-visual-loop commands; \
	else \
		echo "codex-visual-loop not found on PATH; falling back to Python module check"; \
		python3 -m codex_visual_loop_plugin.cli commands; \
	fi

typecheck: ## Run Rust type check when Cargo manifest exists
	@if [[ -f "./$(RUST_MANIFEST)" ]] && command -v cargo >/dev/null 2>&1; then \
		cargo check --manifest-path "./$(RUST_MANIFEST)"; \
	else \
		echo "Skipping typecheck: Rust manifest not found at ./$(RUST_MANIFEST)"; \
	fi

test: ## Run plugin integration tests against Rust backend when available
	@if [[ -f "./$(RUST_MANIFEST)" ]] && command -v cargo >/dev/null 2>&1; then \
		cargo build --manifest-path "./$(RUST_MANIFEST)" >/dev/null; \
		$(RUST_ENV)="$$PWD/$(RUST_BIN_DEBUG)" python3 -m unittest tests/test_codex_visual_loop_plugin.py -v; \
	else \
		python3 -m unittest tests/test_codex_visual_loop_plugin.py -v; \
	fi

uninstall: uninstall-plugin uninstall-skill ## Remove plugin + skill wrapper

uninstall-plugin: ## Uninstall the pip package
	-@python3 -m pip uninstall -y codex-visual-loop-plugin || \
	python3 -m pip uninstall --break-system-packages -y codex-visual-loop-plugin

uninstall-skill: ## Remove installed skill wrapper
	@rm -rf "$(SKILL_DST)"
	@echo "Removed skill from $(SKILL_DST)"
