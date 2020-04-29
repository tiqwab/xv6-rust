#include "../user.h"

void wait_env_id(int pid) {
    // FIXME: avoid spin
    while (sys_wait_env_id(pid) == -E_TRY_AGAIN) {
        __asm__ volatile("pause");
    }
}
