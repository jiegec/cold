#!/bin/bash
set -x -e
make

# helloworld3_asm
RUST_LOG=info cargo run -- --hash-style=both -shared helloworld3_asm_library.o -o helloworld3_asm_library_cold.so
readelf -a helloworld3_asm_library_cold.so > helloworld3_asm_library_cold.so.readelf
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
RUST_LOG=info cargo run -- -dynamic-linker /lib64/ld-linux-x86-64.so.2 helloworld3_asm_main.o helloworld3_asm_library_cold.so -o helloworld3_asm_cold
./helloworld3_asm_cold
[[ $(./helloworld3_asm_cold) =~ "Hello world!" ]] || exit 1

# helloworld4_asm
as helloworld4_asm_syscall.s -o helloworld4_asm_syscall.o
RUST_LOG=info cargo run -- -shared helloworld4_asm_syscall.o -o libhelloworld4_asm_syscall_cold.so
as helloworld4_asm_library.s -o helloworld4_asm_library.o
RUST_LOG=info cargo run -- -shared helloworld4_asm_library.o -L. -lhelloworld4_asm_syscall_cold -o libhelloworld4_asm_library_cold.so
as helloworld4_asm_main.s -o helloworld4_asm_main.o
RUST_LOG=info cargo run -- -dynamic-linker /lib64/ld-linux-x86-64.so.2 helloworld4_asm_main.o -L. -lhelloworld4_asm_library_cold -o helloworld4_asm_cold
./helloworld4_asm_cold
[[ $(./helloworld4_asm_cold) =~ "Hello world!" ]] || exit 1

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

# TODO:
#PATH=../target/debug:$PATH gcc helloworld_c.c -o helloworld_c -v --save-temps

# helloworld4_c
RUST_LOG=info PATH=../target/debug:$PATH gcc -shared -nostdlib helloworld4_asm_syscall.s -o libhelloworld4_c_syscall_cold.so
RUST_LOG=info PATH=../target/debug:$PATH gcc -shared -nostdlib helloworld4_c_library.c -L. -lhelloworld4_c_syscall_cold -o libhelloworld4_c_library_cold.so
RUST_LOG=info PATH=../target/debug:$PATH gcc -nostdlib helloworld4_c_main.c -L. -lhelloworld4_c_library_cold -o helloworld4_c_cold
./helloworld4_c_cold
[[ $(./helloworld4_c_cold) =~ "Hello world!" ]] || exit 1