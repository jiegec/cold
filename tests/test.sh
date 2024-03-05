#!/bin/sh
set -x
./build.sh
RUST_LOG=info cargo run -- helloworld_asm.o -o helloworld_asm_cold
./helloworld_asm_cold