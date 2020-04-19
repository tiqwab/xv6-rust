#include "user.h"

void umain(int argc, char **argv) {
    char *buf1 = (char *) malloc(128);
    if (buf1 == (char *) -1) {
        printf("error when allocating buf1\n");
        return;
    }
    for (int i = 0; i < 127; i++) {
        buf1[i] = 'a' + (i % 26);
    }
    buf1[127] = 0;
    printf("allocated buf1 (%d bytes) at %p\n", 128, buf1);
    printf("buf1: %s\n", buf1);

    char *buf2 = (char *) malloc(128);
    if (buf2 == (char *) -1) {
        printf("error when allocating buf2\n");
        return;
    }
    printf("allocated buf2 (%d bytes) at %p\n", 128, buf2);
    if (buf1 - buf2 != 128 + 8) {
        // +8 is for block header
        printf("the address of buf2 is not that of expected.\n");
        return;
    }
    free(buf2);

    char *buf3 = (char *) malloc(128);
    if (buf3 == (char *) -1) {
        printf("error when allocating buf3\n");
        return;
    }
    printf("allocated buf3 (%d bytes) at %p\n", 128, buf3);
    if (buf3 != buf2) {
        printf("the address of buf3 is not that of expected\n");
        return;
    }

    char *buf4 = (char *) malloc(1024 * 8);
    if (buf4 == (char *) -1) {
        printf("error when allocating buf4\n");
        return;
    }
    printf("allocated buf4 (%d bytes) at %p\n", 1024 * 8, buf4);

    free(buf4);
    free(buf3);
    free(buf1);

    printf("finish malloctest successfully\n");
}
