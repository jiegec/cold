    .section .text
    .globl write
write:
    mov     $1, %rax
    syscall
    ret

    .globl _exit
_exit:
    mov     $60, %rax
    syscall
