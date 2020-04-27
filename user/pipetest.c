#include "user.h"

#define BUF_LEN 128

void umain(int argc, char **argv) {
    int env_id, n;
    char buf[BUF_LEN];
    int fds[2] = {0, 0}; // {fd for read, fd for write}

    if (sys_pipe(fds) < 0) {
        printf("pipetest: cannot create pipe\n");
    }
    printf("fds[0]: %d, fds[1]: %d\n", fds[0], fds[1]);


    if ((env_id = sys_fork()) < 0) {
        printf("pipetest: cannot fork\n");
        return;
    } else if (env_id == 0) {
        // child
        close(fds[0]);

        write(fds[1], "one\n", 4);
        write(fds[1], "two\n", 4);
        write(fds[1], "three\n", 6);

        close(fds[1]);
    } else {
        // parent
        close(fds[1]);

        while ((n = read(fds[0], buf, BUF_LEN)) != 0) {
            buf[n] = '\0';
            printf("received: %s\n", buf);
        }

        if (sys_wait_env_id(env_id) == 0) {
            __asm__ volatile ("pause");
        }

        close(fds[0]);
    }
}
