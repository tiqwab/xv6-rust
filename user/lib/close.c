#include "../user.h"

int close(int fd) {
    return sys_close(fd);
}
