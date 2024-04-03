    .section .bss
buffer:
    .skip 4

    .section .text
    .globl _start
_start:
    mov     $0x65, %rdi
    movl    %edi, buffer(%rip)
    incl    buffer(%rip)

    # write(1, buffer, 1)
    mov     $1, %rdi
    lea     buffer(%rip), %rsi
    mov     $1, %rdx
    mov     $1, %rax
    syscall

    # _exit(0)
    xor     %rdi, %rdi
    mov     $60, %rax
    syscall
