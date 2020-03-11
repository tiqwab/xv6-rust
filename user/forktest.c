#include "user.h"

void umain(int argc, char **argv) {
    int i;
    int child = sys_fork();

    for (i = 0; i < (child ? 10 : 20); i++) {
        printf("%d: I am the %s!\n", i, child ? "parent" : "child");
        sys_yield();
    }
}
