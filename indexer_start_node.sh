#!/bin/bash
set -xe
export RUST_LOG=info
export RUST_BACKTRACE=full

rm -rvf ./data

cargo r --release  -- node \
  --config=./config.toml \
  --datadir="./data" \
  --start=1 \
  --listen-ip="0.0.0.0" \
  --api-port=8544 \
  --http-port=8545 \
  --ws-port=8546
