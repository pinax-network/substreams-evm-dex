.DEFAULT_GOAL := pack

# Reproducible WASM paths and hashes: see build.mk at the repository root.
# substreams pack embeds the imported dex/pools WASM without building it. One
# cargo build per package keeps that WASM, and its module hashes, equal to the
# standalone dex/pools package's.
include ../../build.mk
build: export RUSTFLAGS := $(WASM_RUSTFLAGS)
.PHONY: build
build:
	cargo build --locked --target wasm32-unknown-unknown --release -p dex-pools
	cargo build --locked --target wasm32-unknown-unknown --release -p dex-swaps

# Local packs go to the ignored out/; committed spkgs come from CI's Linux build.
.PHONY: pack
pack: build
	mkdir -p ../../out
	substreams pack -o ../../out/{spkgDefaultName}
