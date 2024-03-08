#!/bin/bash
set -xe
export RUST_LOG=info
export RUST_BACKTRACE=full

cargo r --release -- gen-cfg --config=./config.toml
