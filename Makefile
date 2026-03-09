BINARY := ./target/release/claude-sandbox

.PHONY: build clean test clippy rebuild

build:
	cargo build --release

clean:
	cargo clean

test:
	cargo test

clippy:
	cargo clippy

rebuild: clean build
	-container stop claude-monitor
	-container rm claude-monitor
	container image prune -a
	$(BINARY) init --force
	$(BINARY) build
	$(BINARY) run
