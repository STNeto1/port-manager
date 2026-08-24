.PHONY: build release install test fmt fmt-check lint check clean run daemon list

build:
	cargo build

release:
	cargo build --release

install:
	cargo install --path . --force

test:
	cargo test

fmt:
	cargo fmt

fmt-check:
	cargo fmt --check

lint:
	cargo clippy --all-targets

check: fmt-check lint test

clean:
	cargo clean

run:
	cargo run

daemon:
	cargo run -- daemon

list:
	cargo run -- list
