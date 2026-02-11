all: build test format lint

build:
	cd frontend && npm install && npm run build
	cargo build -p rw

build-release:
	cd frontend && npm install && npm run build
	cargo build --release -p rw --features embed-assets

install:
	cd frontend && npm install && npm run build
	cargo install --path crates/rw --features embed-assets

test:
	cargo llvm-cov --html
	cargo test --doc --workspace
	cd frontend && npm run test

test-e2e:
	cd frontend && npm run test:e2e

format:
	cargo fmt
	cd frontend && npm run format

lint:
	cargo clippy --all-targets
	cd frontend && npm run check

bench:
	cargo bench -p rw-site

bench-baseline:
	cargo bench -p rw-site -- --save-baseline main

bench-compare:
	cargo bench -p rw-site -- --baseline main
