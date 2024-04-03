# https://gist.github.com/adrianratnapala/1321776
    .section .rodata
hello:
    .string "Hello world!\n"


    .section .text
    .globl print
print:
    # write(1, hello, 13)
    mov     $1, %rdi
    # we can no longer use:
    # mov     $hello, %rsi
    # because it fails with: "relocation R_X86_64_32S against `.rodata' can not
    # be used when making a shared object; recompile with -fPIC".
    # thus we need to use PC-relative addressing in shared library:
    lea     hello(%rip), %rsi
    mov     $13, %rdx
    mov     $1, %rax
    syscall
    ret

    .globl exit
exit:
    # _exit(0)
    xor     %rdi, %rdi
    mov     $60, %rax
    syscall
