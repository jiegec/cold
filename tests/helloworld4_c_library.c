void write(int fd, const char *data, int length);
void _exit(int code);
void print() {
    write(1, "Hello world!\n", 13);
}

void exit() {
    _exit(0);
}