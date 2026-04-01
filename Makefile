.PHONY: build test lint fmt check clean rebuild help

## Default target
all: check

## Build in debug mode
build:
	cargo build --all-targets

## Run all tests
test:
	cargo test

## Run clippy with all warnings as errors
lint:
	cargo clippy --all-targets -- -D warnings

## Check formatting without modifying files
fmt:
	cargo fmt --all --check

## Run fmt + lint + test (required before every commit)
check: fmt lint test
	@echo "--- all checks passed"

## Remove build artifacts
clean:
	cargo clean

## Clean, rebuild image, re-init, and run (requires Apple container CLI)
rebuild: clean build
	container image prune -a
	cargo run -- init --force
	cargo run -- build
	cargo run -- run

## Show available targets
help:
	@grep -E '^## ' Makefile | sed 's/## //'
