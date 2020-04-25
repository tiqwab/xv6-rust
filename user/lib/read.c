#include "../user.h"

int read(int fd, char *buf, int count) {
    int r_cnt;

    // FIXME: avoid spin
    while ((r_cnt = sys_read(fd, buf, count)) == -E_TRY_AGAIN) {
        __asm__ volatile("pause");
    }

    return r_cnt;
}
