#!/bin/sh

# helloworld_asm
as helloworld_asm.s -o helloworld_asm.o
ld helloworld_asm.o -o helloworld_asm

readelf -a helloworld_asm.o > helloworld_asm.o.readelf
readelf -a helloworld_asm > helloworld_asm.readelf

# helloworld_c
gcc helloworld_c.c -o helloworld_c -v --save-temps
gcc -static helloworld_c.c -o helloworld_c_static -v --save-temps
gcc -static-pie helloworld_c.c -o helloworld_c_static_pie -v --save-temps

# helloworld2_asm
as helloworld2_asm1.s -o helloworld2_asm1.o
as helloworld2_asm2.s -o helloworld2_asm2.o
ld helloworld2_asm1.o helloworld2_asm2.o -o helloworld2_asm

readelf -a helloworld2_asm > helloworld2_asm.readelf

# helloworld3_asm
as helloworld3_asm_library.s -o helloworld3_asm_library.o
ld -shared helloworld3_asm_library.o -o helloworld3_asm_library.so
as helloworld3_asm_main.s -o helloworld3_asm_main.o
ld -dynamic-linker /lib64/ld-linux-x86-64.so.2 helloworld3_asm_main.o helloworld3_asm_library.so -o helloworld3_asm

# uname_asm
as uname_asm.s -o uname_asm.o
ld uname_asm.o -o uname_asm
readelf -a uname_asm > uname_asm.readelf

# bss_asm
as bss_asm.s -o bss_asm.o
ld bss_asm.o -o bss_asm
readelf -a bss_asm > bss_asm.readelf