#!/bin/bash
set -xe
export RUST_LOG=info
export RUST_BACKTRACE=full

cargo r --release --bin biterc -- \
  --config=./config.toml \
  --datadir="./data" \
  --listen="0.0.0.0" \
  --api-port=8544 \
  --http-port=8545 \
  --ws-port=8546
