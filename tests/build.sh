#!/bin/sh
as helloworld_asm.s -o helloworld_asm.o
ld helloworld_asm.o -o helloworld_asm
gcc helloworld_c.c -o helloworld_c -v --save-temps
strip helloworld_asm

readelf -a helloworld_asm.o > helloworld_asm.o.readelf
readelf -a helloworld_asm > helloworld_asm.readelf