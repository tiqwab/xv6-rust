#include "user.h"

void echo(int n, char **msg) {
    if (n < 0) return;
    printf("%s", msg[0]);

    for (int i = 1; i < n; i++) {
        printf(" %s", msg[i]);
    }

    printf("\n");
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
