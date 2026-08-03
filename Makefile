all: build test format lint

dev:
	npm -w @rwdocs/viewer run dev

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
	npm run format

lint:
	cargo fmt --all -- --check
	cargo clippy --all-targets -- -D warnings
	npm -w @rwdocs/viewer run check
	npm -w @rwdocs/viewer run lint
	npm run lint
	npx -w @rwdocs/viewer prettier --check .
	npx prettier --check .
	npm -w @rwdocs/viewer run check:pack
	# `rw backstage publish` resolves PlantUML includes through rw-plantuml so a
	# publish-only binary never links the diagram renderer. Nothing in the type
	# system enforces that; a stray `use rw_kroki::` would restore it silently.
	# The tree is captured first: piped straight into grep, a cargo failure
	# produces no match and the negation turns that into a pass.
	tree=$$(cargo tree -p rw-storage-s3 --features publish -e normal) && ! echo "$$tree" | grep -q rw-kroki

audit:
	cargo deny check licenses sources

version:
	@test -n "$(VERSION)" || (echo "Usage: make version VERSION=0.2.0" && exit 1)

	cargo set-version --workspace $(VERSION)
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
