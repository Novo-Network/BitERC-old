#!/bin/bash
set -xe
export RUST_LOG=info
export RUST_BACKTRACE=full

cargo r --release --bin indexer -- \
  --config=./config.toml \
  --datadir="./data" \
  --listen-ip="0.0.0.0" \
  --api-port=8544 \
  --http-port=8545 \
  --ws-port=8546
