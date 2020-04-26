#include "user.h"

void umain(int argc, char **argv) {
    int fds[2] = {0, 0};
    if (sys_pipe(fds) < 0) {
        printf("pipetest: cannot create pipe\n");
    }
    printf("fds[0]: %d, fds[1]: %d\n", fds[0], fds[1]);
}
