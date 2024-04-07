OUT=helloworld_asm \
	helloworld_asm.o.readelf \
	helloworld_asm.readelf \
	helloworld_c \
	helloworld_c_static \
	helloworld_c_static_pie \
	helloworld2_asm1.o \
	helloworld2_asm2.o \
	helloworld2_asm \
	helloworld2_asm.readelf \
	helloworld3_asm_library.o \
	libhelloworld3_asm_library.so \
	helloworld3_asm_main.o \
	helloworld3_asm \
	helloworld3_asm_pie \
	helloworld4_asm_syscall.o \
	libhelloworld4_asm_syscall.so \
	helloworld4_asm_library.o \
	libhelloworld4_asm_library.so \
	helloworld4_asm \
	uname_asm.o \
	uname_asm \
	uname_asm.readelf \
	bss_asm.o \
	bss_asm \
	bss_asm.readelf \
	helloworld4_c

all: $(OUT)

clean:
	rm -f *.o *.readelf $(OUT)

helloworld_asm: helloworld_asm.o
	ld helloworld_asm.o -o helloworld_asm

%.readelf: %
	readelf -a $^ > $@

helloworld_c: helloworld_c.c
	gcc helloworld_c.c -o helloworld_c

helloworld_c_static: helloworld_c.c
	gcc -static helloworld_c.c -o helloworld_c_static

helloworld_c_static_pie: helloworld_c.c
	gcc -static-pie helloworld_c.c -o helloworld_c_static_pie

helloworld2_asm: helloworld2_asm1.o helloworld2_asm2.o
	ld helloworld2_asm1.o helloworld2_asm2.o -o helloworld2_asm

libhelloworld3_asm_library.so: helloworld3_asm_library.o
	ld -shared helloworld3_asm_library.o -o libhelloworld3_asm_library.so

helloworld3_asm: helloworld3_asm_main.o libhelloworld3_asm_library.so
	ld -dynamic-linker /lib64/ld-linux-x86-64.so.2 helloworld3_asm_main.o -L. -lhelloworld3_asm_library -o helloworld3_asm

helloworld3_asm_pie: helloworld3_asm_main.o libhelloworld3_asm_library.so
	ld -pie -dynamic-linker /lib64/ld-linux-x86-64.so.2 helloworld3_asm_main.o -L. -lhelloworld3_asm_library -o helloworld3_asm_pie

libhelloworld4_asm_syscall.so: helloworld4_asm_syscall.o
	ld -shared helloworld4_asm_syscall.o -o libhelloworld4_asm_syscall.so

libhelloworld4_asm_library.so: helloworld4_asm_library.o libhelloworld4_asm_syscall.so
	ld -shared helloworld4_asm_library.o -L. -lhelloworld4_asm_syscall -o libhelloworld4_asm_library.so

helloworld4_asm: helloworld4_asm_main.o libhelloworld4_asm_library.so
	ld -dynamic-linker /lib64/ld-linux-x86-64.so.2 helloworld4_asm_main.o -rpath-link . -L. -lhelloworld4_asm_library -o helloworld4_asm

uname_asm: uname_asm.o
	ld uname_asm.o -o uname_asm

bss_asm: bss_asm.o
	ld bss_asm.o -o bss_asm

libhelloworld4_asm_syscall.so: helloworld4_asm_syscall.o
	ld -shared helloworld4_asm_syscall.o -o libhelloworld4_asm_syscall.so

libhelloworld4_c_library.so: helloworld4_c_library.o libhelloworld4_asm_syscall.so
	ld -shared helloworld4_c_library.o -L. -lhelloworld4_asm_syscall -o libhelloworld4_c_library.so

helloworld4_c: libhelloworld4_c_library.so helloworld4_c_main.o
	ld -dynamic-linker /lib64/ld-linux-x86-64.so.2 helloworld4_c_main.o -rpath-link . -L. -lhelloworld4_c_library -o helloworld4_c