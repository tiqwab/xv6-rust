#include "../user.h"

int open(char *path, int mode) {
    return sys_open(path, mode);
}
