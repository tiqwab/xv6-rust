// The printf implementation is based on `lib/printf.c` in JOS.
// See COPYRIGHT for copyright information.

#include "../user.h"

// Collect up to 256 characters into a buffer
// and perform ONE system call to print all of them,
// in order to make the lines output to the console atomic
// and prevent interrupts from causing context switches
// in the middle of a console output line and such.
struct printbuf {
    int idx;       // current buffer index
    int cnt;       // total bytes printed so far
    char buf[256];
};

static void putch(int ch, struct printbuf *b) {
    b->buf[b->idx++] = ch;
    if (b->idx == 256-1) {
        sys_cputs(b->buf, b->idx);
        b->idx = 0;
    }
    b->cnt++;
}

int vcprintf(const char *fmt, va_list ap) {
    struct printbuf b;

    b.idx = 0;
    b.cnt = 0;
    vprintfmt((void*)putch, &b, fmt, ap);
    sys_cputs(b.buf, b.idx);

    return b.cnt;
}

int printf(const char *fmt, ...) {
    va_list ap;
    int cnt;

    va_start(ap, fmt);
    cnt = vcprintf(fmt, ap);
    va_end(ap);

    return cnt;
}
