# https://gist.github.com/adrianratnapala/1321776
    .section .rodata
hello:
    .string "Hello world!\n"


    .section .text
    .globl print
print:
    # write(1, hello, 13)
    mov     $1, %rdi
    lea     hello(%rip), %rsi
    mov     $13, %rdx
    call    write
    ret

    .globl exit
exit:
    # _exit(0)
    xor     %rdi, %rdi
    call _exit
