#!/bin/bash
set -x -e
./build.sh

# helloworld_asm
RUST_LOG=info cargo run -- helloworld_asm.o -o helloworld_asm_cold
./helloworld_asm_cold
[[ $(./helloworld_asm_cold) == "Hello world!" ]] || exit 1
readelf -a helloworld_asm_cold > helloworld_asm_cold.readelf

# helloworld2_asm
RUST_LOG=info cargo run -- helloworld2_asm1.o helloworld2_asm2.o -o helloworld2_asm_cold
./helloworld2_asm_cold
[[ $(./helloworld2_asm_cold) == "Hello world!" ]] || exit 1
readelf -a helloworld2_asm_cold > helloworld2_asm_cold.readelf

# reversed order
RUST_LOG=info cargo run -- helloworld2_asm2.o helloworld2_asm1.o -o helloworld2_asm_cold_rev
./helloworld2_asm_cold_rev
[[ $(./helloworld2_asm_cold_rev) == "Hello world!" ]] || exit 1
readelf -a helloworld2_asm_cold_rev > helloworld2_asm_cold_rev.readelf

# helloworld3_asm
RUST_LOG=info cargo run -- --hash-style=both -shared helloworld3_asm_library.o -o helloworld3_asm_library_cold.so
readelf -a helloworld3_asm_library.so > helloworld3_asm_library.so.readelf
ld -dynamic-linker /lib64/ld-linux-x86-64.so.2 helloworld3_asm_main.o helloworld3_asm_library_cold.so -o helloworld3_asm_cold
readelf -a helloworld3_asm_cold > helloworld3_asm_cold.readelf
export LD_LIBRARY_PATH=$PWD 
./helloworld3_asm_cold
[[ $(./helloworld3_asm_cold) =~ "Hello world!" ]] || exit 1
RUST_LOG=info cargo run -- --hash-style=sysv -shared helloworld3_asm_library.o -o helloworld3_asm_library_cold.so
ld -dynamic-linker /lib64/ld-linux-x86-64.so.2 helloworld3_asm_main.o helloworld3_asm_library_cold.so -o helloworld3_asm_cold
./helloworld3_asm_cold
[[ $(./helloworld3_asm_cold) =~ "Hello world!" ]] || exit 1
RUST_LOG=info cargo run -- --hash-style=gnu -shared helloworld3_asm_library.o -o helloworld3_asm_library_cold.so
ld -dynamic-linker /lib64/ld-linux-x86-64.so.2 helloworld3_asm_main.o helloworld3_asm_library_cold.so -o helloworld3_asm_cold
./helloworld3_asm_cold
[[ $(./helloworld3_asm_cold) =~ "Hello world!" ]] || exit 1
RUST_LOG=info cargo run -- -soname test.so -shared helloworld3_asm_library.o -o helloworld3_asm_library_cold.so
ld -dynamic-linker /lib64/ld-linux-x86-64.so.2 helloworld3_asm_main.o helloworld3_asm_library_cold.so -o helloworld3_asm_cold
ln -sf helloworld3_asm_library_cold.so test.so
./helloworld3_asm_cold
[[ $(./helloworld3_asm_cold) =~ "Hello world!" ]] || exit 1

# uname_asm
RUST_LOG=info cargo run -- uname_asm.o -o uname_asm_cold
./uname_asm_cold
[[ $(./uname_asm_cold) =~ "Linux" ]] || exit 1
readelf -a uname_asm_cold > uname_asm_cold.readelf

# bss_asm
RUST_LOG=info cargo run -- bss_asm.o -o bss_asm_cold
./bss_asm_cold
[[ $(./bss_asm_cold) =~ "f" ]] || exit 1
readelf -a bss_asm_cold > bss_asm_cold.readelf