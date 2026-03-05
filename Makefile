all: build test format lint

build:
	npm install
	npm -w @rwdocs/viewer run build
	npm -w @rwdocs/viewer run build:lib
	cargo build -p rw
	npm -w @rwdocs/core run build

build-release:
	npm install
	npm -w @rwdocs/viewer run build
	npm -w @rwdocs/viewer run build:lib
	cargo build --release -p rw --features embed-assets
	npm -w @rwdocs/core run build

install:
	npm install
	npm -w @rwdocs/viewer run build
	npm -w @rwdocs/viewer run build:lib
	cargo install --path crates/rw --features embed-assets

test:
	cargo llvm-cov --html
	cargo test --doc --workspace
	npm -w @rwdocs/viewer run test

test-e2e:
	npm -w @rwdocs/viewer run test:e2e

format:
	cargo fmt
	npm -w @rwdocs/viewer run format

lint:
	cargo clippy --all-targets
	npm -w @rwdocs/viewer run check
	npm -w @rwdocs/viewer run lint

version:
	@test -n "$(VERSION)" || (echo "Usage: make version VERSION=0.2.0" && exit 1)

	cargo set-version --workspace $(VERSION)
	cargo set-version --manifest-path crates/rw-napi/Cargo.toml $(VERSION)
	cargo update -w

	cd packages/core && npm version $(VERSION) --no-git-tag-version && npx napi version
	cd packages/viewer && npm version $(VERSION) --no-git-tag-version
	npm install

	$(MAKE) build

bench:
	cargo bench -p rw-site

bench-baseline:
	cargo bench -p rw-site -- --save-baseline main

bench-compare:
	cargo bench -p rw-site -- --baseline main
