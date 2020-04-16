#include "user.h"

void umain(int argc, char **argv) {
    sys_mknod("console", 1, 1);
    int fd = open("console", O_RDWR); // stdin
    sys_dup(fd); // stdout
    sys_dup(fd); // stderr

    int child = sys_fork();
    if (child < 0) {
        printf("Error in fork\n");
        return;
    } else if (child == 0) {
        // child
        sys_exec("/filetest");
    } else {
        // parent
        while (1) {
            __asm__ volatile("pause");
        }
    }
}
