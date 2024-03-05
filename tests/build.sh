#!/bin/sh

# helloworld_asm
as helloworld_asm.s -o helloworld_asm.o
ld helloworld_asm.o -o helloworld_asm

readelf -a helloworld_asm.o > helloworld_asm.o.readelf
readelf -a helloworld_asm > helloworld_asm.readelf

# helloworld_c
gcc helloworld_c.c -o helloworld_c -v --save-temps

# helloworld2_asm
as helloworld2_asm1.s -o helloworld2_asm1.o
as helloworld2_asm2.s -o helloworld2_asm2.o
ld helloworld2_asm1.o helloworld2_asm2.o -o helloworld2_asm

readelf -a helloworld2_asm > helloworld2_asm.readelf