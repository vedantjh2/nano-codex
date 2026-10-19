.PHONY: install build check
CARGO := $(shell command -v cargo 2>/dev/null || printf '%s/.cargo/bin/cargo' "$(HOME)")
install:
	./scripts/install.sh

build:
	$(CARGO) build --locked

check:
	$(CARGO) fmt --check
	$(CARGO) clippy --locked --all-targets -- -D warnings
	$(CARGO) test --locked
