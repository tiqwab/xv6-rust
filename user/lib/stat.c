#include "../user.h"

int stat(char *path, struct stat *statbuf) {
    int fd = open(path, O_RDONLY);
    if (fd < 0) {
        return -1;
    }
    int res = sys_fstat(fd, statbuf);
    close(fd);
    return res;
}

