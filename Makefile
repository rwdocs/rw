all: build test format lint

build:
	cd frontend && npm install && npm run build
	cargo build -p docstage

build-release:
	cd frontend && npm install && npm run build
	cargo build --release -p docstage --features embed-assets

install:
	cd frontend && npm install && npm run build
	cargo install --path crates/docstage --features embed-assets

test:
	cargo llvm-cov --html
	cd frontend && npm run test

test-e2e:
	cd frontend && npm run test:e2e

format:
	cargo fmt
	cd frontend && npm run format

lint:
	cargo clippy --all-targets -- -W clippy::pedantic
	cd frontend && npm run check
