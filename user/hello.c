#include "user.h"

void umain(int argc, char **argv) {
    printf("hello world\n");
    // sys_cputs((const char *) 0xf0000000, 12); // to check user_mem_assert
    for (;;) { }
}
