all: build test format lint

build:
	cd frontend && npm install && npm run build:bundle
	uv sync --reinstall

test:
	cargo llvm-cov --html
	uv run pytest
	cd frontend && npm run test

test-e2e:
	cd frontend && npm run test:e2e

format:
	cargo fmt
	uv run ruff format .
	cd frontend && npm run format

lint:
	cargo clippy --all-targets -- -W clippy::pedantic
	uv run ruff check .
	uv run mypy .
	cd frontend && npm run check
