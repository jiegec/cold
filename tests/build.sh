#!/bin/sh
as helloworld_asm.s -o helloworld_asm.o
ld helloworld_asm.o -o helloworld_asm
gcc helloworld_c.c -o helloworld_c -v --save-temps
strip helloworld_asm