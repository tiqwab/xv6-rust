// Test preemption by forking off a child process that just spins forever.
// Let it run for a couple time slices, then kill it.

#include "user.h"

void
umain(int argc, char **argv)
{
    int env;

    printf("I am the parent.  Forking the child...\n");
    if ((env = sys_fork()) == 0) {
        printf("I am the child.  Spinning...\n");
        while (1)
            /* do nothing */;
    }

    printf("I am the parent.  Running the child...\n");
    sys_yield();
    sys_yield();
    sys_yield();
    sys_yield();
    sys_yield();
    sys_yield();
    sys_yield();
    sys_yield();

    printf("I am the parent.  Killing the child...\n");
    sys_kill(env);
}

