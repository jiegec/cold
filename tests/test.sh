#!/bin/bash
set -x -e
./build.sh

RUST_LOG=info cargo run -- helloworld_asm.o -o helloworld_asm_cold
./helloworld_asm_cold
[[ $(./helloworld_asm_cold) == "Hello world!" ]] || exit 1

RUST_LOG=info cargo run -- helloworld2_asm1.o helloworld2_asm2.o -o helloworld2_asm_cold
./helloworld2_asm_cold
[[ $(./helloworld2_asm_cold) == "Hello world!" ]] || exit 1