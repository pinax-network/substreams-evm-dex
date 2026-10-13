.DEFAULT_GOAL := pack

# substreams pack embeds the imported packages' WASM without building it.
# One cargo build per package: built together, substreams-database-change
# turns on buffa's json feature for the imported maps too, so their WASM and
# module hashes would differ from the dex/pools and dex/swaps packages'.
# Reproducible WASM paths and hashes: see build.mk at the repository root.
include ../build.mk
build: export RUSTFLAGS := $(WASM_RUSTFLAGS)
.PHONY: build
build:
	cargo build --locked --target wasm32-unknown-unknown --release -p dex-pools
	cargo build --locked --target wasm32-unknown-unknown --release -p dex-swaps
	cargo build --locked --target wasm32-unknown-unknown --release -p evm-dex

# The sink reads one schema file: the clickhouse/schema.*.sql files in byte
# order, as substreams-evm's evm-dex/clickhouse Makefile concatenated them.
.PHONY: schema
schema:
	@echo "-- This file is generated. Do not edit." > clickhouse/schema.sql
	@echo "" >> clickhouse/schema.sql
	@for f in $$(ls clickhouse/schema.*.sql 2>/dev/null | LC_ALL=C sort); do \
		cat "$$f" >> clickhouse/schema.sql; \
		echo "" >> clickhouse/schema.sql; \
	done

# Local packs go to the ignored out/; committed spkgs come from CI's Linux build.
.PHONY: pack
pack: build schema
	mkdir -p ../out
	substreams pack -o ../out/{spkgDefaultName}
