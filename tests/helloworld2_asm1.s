    .section .rodata
random:
    .string "This is not hello world\n"


    .section .text
    .globl _start
_start:
    call print
    call exit
    # should not reach here
    call print
