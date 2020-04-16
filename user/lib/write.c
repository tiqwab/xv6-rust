#include "../user.h"

int write(int fd, char *buf, int count) {
    return sys_write(fd, buf, count);
}
