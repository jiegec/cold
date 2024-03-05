#!/bin/bash
set -x -e
./build.sh

RUST_LOG=info cargo run -- helloworld_asm.o -o helloworld_asm_cold
[[ $(./helloworld_asm_cold) == "Hello world!" ]] || exit 1
readelf -a helloworld_asm_cold > helloworld_asm_cold.readelf

RUST_LOG=info cargo run -- helloworld2_asm1.o helloworld2_asm2.o -o helloworld2_asm_cold
[[ $(./helloworld2_asm_cold) == "Hello world!" ]] || exit 1
readelf -a helloworld2_asm_cold > helloworld2_asm_cold.readelf

# reversed order
RUST_LOG=info cargo run -- helloworld2_asm2.o helloworld2_asm1.o -o helloworld2_asm_cold_rev
[[ $(./helloworld2_asm_cold_rev) == "Hello world!" ]] || exit 1
readelf -a helloworld2_asm_cold_rev > helloworld2_asm_cold_rev.readelf

RUST_LOG=info cargo run -- uname_asm.o -o uname_asm_cold
[[ $(./uname_asm_cold) =~ "Linux" ]] || exit 1
readelf -a uname_asm_cold > uname_asm_cold.readelf