    .section .data
buffer:
sysname:
    .zero 65
nodename:
    .zero 65
release:
    .zero 65
version:
    .zero 65
machine:
    .zero 65
domainname:
    .zero 65

    .section .rodata
newline:
    .string "\n"

    .section .text

    .globl uname
uname:
    # uname(buffer)
    mov     $buffer, %rdi
    mov     $63, %rax
    syscall
    ret

    .globl print
print:
    # strlen(arg1)
    mov     $0, %rdx
    cmpb    $0, (%rdi)
    je      _end_loop

_loop:
    add     $1, %rdx
    cmpb    $0, (%rdi, %rdx, 1)
    jne     _loop
_end_loop:

    # write(1, arg1, strlen(arg1))
    mov     %rdi, %rsi
    mov     $1, %rdi
    mov     $1, %rax
    syscall

    # write(1, newline, 1)
    mov     $1, %rdi
    mov     $newline, %rsi
    mov     $1, %rdx
    mov     $1, %rax
    syscall

    ret

    .globl exit

exit:
    # _exit(0)
    xor     %rdi, %rdi
    mov     $60, %rax
    syscall

    .globl _start
_start:
    call uname

    mov     $sysname, %rdi
    call print

    mov     $nodename, %rdi
    call print

    mov     $release, %rdi
    call print

    mov     $version, %rdi
    call print

    mov     $machine, %rdi
    call print

    mov     $domainname, %rdi
    call print

    call exit

