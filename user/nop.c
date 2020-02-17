void umain(int argc, char **argv) {
    __asm__ volatile ("movl $0, %edx; movl $1, %eax; movl $0, %ecx;  div %ecx;");
    // __asm__ volatile("int $48");
    for (;;) { }
}
