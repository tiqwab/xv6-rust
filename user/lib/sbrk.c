#include "../user.h"

void *sbrk(unsigned int nbytes) {
    return sys_sbrk(nbytes);
}
