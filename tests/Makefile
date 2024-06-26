# https://stackoverflow.com/questions/589276/how-can-i-use-bash-syntax-in-makefile-targets
SHELL := /bin/bash

OUT=helloworld_asm \
	helloworld_asm.o.readelf \
	helloworld_asm.readelf \
	helloworld_c \
	helloworld_c_static \
	helloworld_c_static_pie \
	helloworld2_asm1.o \
	helloworld2_asm2.o \
	helloworld2_asm \
	helloworld2_asm.readelf \
	helloworld3_asm_library.o \
	libhelloworld3_asm_library.so \
	helloworld3_asm_main.o \
	helloworld3_asm \
	helloworld3_asm_cold \
	helloworld3_asm_pie \
	helloworld4_asm_syscall.o \
	libhelloworld4_asm_syscall.so \
	helloworld4_asm_library.o \
	libhelloworld4_asm_library.so \
	helloworld4_asm \
	uname_asm.o \
	uname_asm \
	uname_asm.readelf \
	bss_asm.o \
	bss_asm \
	bss_asm.readelf \
	helloworld4_c \
	helloworld_asm_cold \
	helloworld_asm_cold.readelf \
	helloworld2_asm_cold \
	helloworld2_asm_cold_rev \
	libhelloworld3_asm_library_cold.so \
	helloworld3_asm_cold_soname \
	helloworld3_asm_pie_cold \
	helloworld4_asm_cold \
	helloworld4_c_cold \
	uname_asm_cold \
	bss_asm_cold

all: $(OUT)

clean:
	rm -f *.o *.readelf $(OUT)

helloworld_asm: helloworld_asm.o
	ld helloworld_asm.o -o helloworld_asm

%.readelf: %
	readelf -a $^ > $@

helloworld_c: helloworld_c.c
	gcc helloworld_c.c -o helloworld_c

helloworld_c_static: helloworld_c.c
	gcc -static helloworld_c.c -o helloworld_c_static

helloworld_c_static_pie: helloworld_c.c
	gcc -static-pie helloworld_c.c -o helloworld_c_static_pie

helloworld2_asm: helloworld2_asm1.o helloworld2_asm2.o
	ld helloworld2_asm1.o helloworld2_asm2.o -o helloworld2_asm

libhelloworld3_asm_library.so: helloworld3_asm_library.o
	ld -shared helloworld3_asm_library.o -o libhelloworld3_asm_library.so

helloworld3_asm: helloworld3_asm_main.o libhelloworld3_asm_library.so
	ld -dynamic-linker /lib64/ld-linux-x86-64.so.2 helloworld3_asm_main.o -L. -lhelloworld3_asm_library -o helloworld3_asm

helloworld3_asm_pie: helloworld3_asm_main.o libhelloworld3_asm_library.so
	ld -pie -dynamic-linker /lib64/ld-linux-x86-64.so.2 helloworld3_asm_main.o -L. -lhelloworld3_asm_library -o helloworld3_asm_pie

libhelloworld4_asm_syscall.so: helloworld4_asm_syscall.o
	ld -shared helloworld4_asm_syscall.o -o libhelloworld4_asm_syscall.so

libhelloworld4_asm_library.so: helloworld4_asm_library.o libhelloworld4_asm_syscall.so
	ld -shared helloworld4_asm_library.o -L. -lhelloworld4_asm_syscall -o libhelloworld4_asm_library.so

helloworld4_asm: helloworld4_asm_main.o libhelloworld4_asm_library.so
	ld -dynamic-linker /lib64/ld-linux-x86-64.so.2 helloworld4_asm_main.o -rpath-link . -L. -lhelloworld4_asm_library -o helloworld4_asm

uname_asm: uname_asm.o
	ld uname_asm.o -o uname_asm

bss_asm: bss_asm.o
	ld bss_asm.o -o bss_asm

libhelloworld4_c_library.so: helloworld4_c_library.o libhelloworld4_asm_syscall.so
	ld -shared helloworld4_c_library.o -L. -lhelloworld4_asm_syscall -o libhelloworld4_c_library.so

helloworld4_c: libhelloworld4_c_library.so helloworld4_c_main.o
	ld -dynamic-linker /lib64/ld-linux-x86-64.so.2 helloworld4_c_main.o -rpath-link . -L. -lhelloworld4_c_library -o helloworld4_c

# cold linker
helloworld_asm_cold: helloworld_asm.o
	RUST_LOG=info cargo run -- helloworld_asm.o -o helloworld_asm_cold

helloworld2_asm_cold: helloworld2_asm1.o helloworld2_asm2.o
	RUST_LOG=info cargo run -- helloworld2_asm1.o helloworld2_asm2.o -o helloworld2_asm_cold

helloworld2_asm_cold_rev: helloworld2_asm1.o helloworld2_asm2.o
	RUST_LOG=info cargo run -- helloworld2_asm2.o helloworld2_asm1.o -o helloworld2_asm_cold_rev

libhelloworld3_asm_library_cold.so: helloworld3_asm_library.o
	RUST_LOG=info cargo run -- -shared helloworld3_asm_library.o -o libhelloworld3_asm_library_cold.so

helloworld3_asm_cold: helloworld3_asm_main.o libhelloworld3_asm_library_cold.so
	ld -dynamic-linker /lib64/ld-linux-x86-64.so.2 helloworld3_asm_main.o libhelloworld3_asm_library_cold.so -o helloworld3_asm_cold

helloworld3_asm_pie_cold: helloworld3_asm_main.o libhelloworld3_asm_library_cold.so
	RUST_LOG=info cargo run -- -pie -dynamic-linker /lib64/ld-linux-x86-64.so.2 helloworld3_asm_main.o -L. -lhelloworld3_asm_library_cold -o helloworld3_asm_pie_cold

libhelloworld3_asm_library_cold_soname.so: helloworld3_asm_library.o
	RUST_LOG=info cargo run -- -soname test.so -shared helloworld3_asm_library.o -o libhelloworld3_asm_library_cold_soname.so

test.so: libhelloworld3_asm_library_cold_soname.so
	ln -sf libhelloworld3_asm_library_cold_soname.so test.so

helloworld3_asm_cold_soname: helloworld3_asm_main.o libhelloworld3_asm_library_cold_soname.so test.so
	ld -dynamic-linker /lib64/ld-linux-x86-64.so.2 helloworld3_asm_main.o libhelloworld3_asm_library_cold_soname.so -o helloworld3_asm_cold_soname

libhelloworld4_asm_syscall_cold.so: helloworld4_asm_syscall.o
	RUST_LOG=info cargo run -- -shared helloworld4_asm_syscall.o -o libhelloworld4_asm_syscall_cold.so

libhelloworld4_asm_library_cold.so: helloworld4_asm_library.o
	RUST_LOG=info cargo run -- -shared helloworld4_asm_library.o -L. -lhelloworld4_asm_syscall_cold -o libhelloworld4_asm_library_cold.so

helloworld4_asm_cold: libhelloworld4_asm_syscall_cold.so libhelloworld4_asm_library_cold.so helloworld4_asm_main.o
	RUST_LOG=info cargo run -- -dynamic-linker /lib64/ld-linux-x86-64.so.2 helloworld4_asm_main.o -L. -lhelloworld4_asm_library_cold -o helloworld4_asm_cold

libhelloworld4_c_syscall_cold.so: helloworld4_asm_syscall.s
	RUST_LOG=info PATH=../target/debug:$(PATH) gcc -shared -nostdlib helloworld4_asm_syscall.s -o libhelloworld4_c_syscall_cold.so

libhelloworld4_c_library_cold.so: helloworld4_c_library.c libhelloworld4_c_syscall_cold.so
	RUST_LOG=info PATH=../target/debug:$(PATH) gcc -shared -nostdlib helloworld4_c_library.c -L. -lhelloworld4_c_syscall_cold -o libhelloworld4_c_library_cold.so

helloworld4_c_cold: helloworld4_c_main.c libhelloworld4_c_library_cold.so
	RUST_LOG=info PATH=../target/debug:$(PATH) gcc -nostdlib helloworld4_c_main.c -L. -lhelloworld4_c_library_cold -o helloworld4_c_cold

uname_asm_cold: uname_asm.o
	RUST_LOG=info cargo run -- uname_asm.o -o uname_asm_cold

bss_asm_cold: bss_asm.o
	RUST_LOG=info cargo run -- bss_asm.o -o bss_asm_cold

check: export LD_LIBRARY_PATH = $(PWD)
check: all
	# helloworld_asm
	./helloworld_asm | grep -x "Hello world!" || exit 1
	./helloworld_asm_cold | grep -x "Hello world!" || exit 1

	# helloworld2_asm
	./helloworld2_asm | grep -x "Hello world!" || exit 1
	./helloworld2_asm_cold | grep -x "Hello world!" || exit 1
	./helloworld2_asm_cold_rev | grep -x "Hello world!" || exit 1

	# helloworld3_asm
	./helloworld3_asm | grep -x "Hello world!" || exit 1
	./helloworld3_asm_pie | grep -x "Hello world!" || exit 1
	./helloworld3_asm_pie_cold | grep -x "Hello world!" || exit 1
	./helloworld3_asm_cold | grep -x "Hello world!" || exit 1
	./helloworld3_asm_cold_soname | grep -x "Hello world!" || exit 1

	# helloworld4_asm
	./helloworld4_asm | grep -x "Hello world!" || exit 1
	./helloworld4_asm_cold | grep -x "Hello world!" || exit 1

	# helloworld4_c
	./helloworld4_c | grep -x "Hello world!" || exit 1
	./helloworld4_c_cold | grep -x "Hello world!" || exit 1

	# uname_asm
	./uname_asm | grep -x "Linux" || exit 1
	./uname_asm_cold | grep -x "Linux" || exit 1

	# bss_asm
	./bss_asm | grep -x "f" || exit 1
	./bss_asm_cold | grep -x "f" || exit 1
