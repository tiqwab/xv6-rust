#include "user.h"

void echo(int n, char **msg) {
    if (n < 0) return;
    write(STDOUT_FILENO, msg[0], strlen(msg[0]));

    for (int i = 1; i < n; i++) {
        write(STDOUT_FILENO, " ", 1);
        write(STDOUT_FILENO, msg[i], strlen(msg[i]));
    }

    write(STDOUT_FILENO, "\n", 1);
}

void umain(int argc, char **argv) {
    if (argc < 2) {
        char *msg[] = {""};
        echo(1, msg);
    } else {
        char **msg = &argv[1];
        echo(argc - 1, msg);
    }
}
