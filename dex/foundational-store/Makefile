.DEFAULT_GOAL := pack

# The per-protocol packages map_entries imports; substreams pack embeds their
# WASM without building it.
IMPORTS := sunpump balancer bancor curvefi aerodrome traderjoe kyber-elastic uniswap-v1 uniswap-v2 uniswap-v3 uniswap-v4

# Reproducible WASM paths and hashes: see build.mk at the repository root.
# One cargo build per package keeps each imported WASM, and its module hash,
# equal to the standalone package's.
include ../../build.mk
build: export RUSTFLAGS := $(WASM_RUSTFLAGS)
.PHONY: build
build:
	for package in $(IMPORTS) evm-dex-foundational-store; do \
		cargo build --locked --target wasm32-unknown-unknown --release -p $$package || exit 1; \
	done

# Local packs go to the ignored out/; committed spkgs come from CI's Linux build.
.PHONY: pack
pack: build
	mkdir -p ../../out
	substreams pack -o ../../out/{spkgDefaultName}
