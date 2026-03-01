all: build test format lint

build:
	yarn install
	yarn workspace @rwdocs/viewer run build
	cargo build -p rw
	yarn workspace @rwdocs/core run build

build-release:
	yarn install
	yarn workspace @rwdocs/viewer run build
	cargo build --release -p rw --features embed-assets
	yarn workspace @rwdocs/core run build

install:
	yarn install
	yarn workspace @rwdocs/viewer run build
	cargo install --path crates/rw --features embed-assets

test:
	cargo llvm-cov --html
	cargo test --doc --workspace
	yarn workspace @rwdocs/viewer run test

test-e2e:
	yarn workspace @rwdocs/viewer run test:e2e

format:
	cargo fmt
	yarn workspace @rwdocs/viewer run format

lint:
	cargo clippy --all-targets
	yarn workspace @rwdocs/viewer run check
	yarn workspace @rwdocs/viewer run lint

version:
	@test -n "$(VERSION)" || (echo "Usage: make version VERSION=0.2.0" && exit 1)
	
	cargo set-version --workspace $(VERSION)
	cargo set-version --manifest-path crates/rw-napi/Cargo.toml $(VERSION)
	cargo generate-lockfile
	
	cd packages/core && npm version $(VERSION) --no-git-tag-version && npx napi version
	cd packages/viewer && npm version $(VERSION) --no-git-tag-version
	yarn install

bench:
	cargo bench -p rw-site

bench-baseline:
	cargo bench -p rw-site -- --save-baseline main

bench-compare:
	cargo bench -p rw-site -- --baseline main
