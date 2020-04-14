// This file comes from lib/syscall.c in jos. See COPYRIGHT for copyright information.

#include "../user.h"

// TODO: share with kernel
#define T_SYSCALL 0x30

// TODO: share with kernel
#define SYS_CPUTS 0
#define SYS_GETC 1
#define SYS_EXIT 2
#define SYS_YIELD 3
#define SYS_GET_ENV_ID 4
#define SYS_FORK 5
#define SYS_KILL 6
#define SYS_EXEC 7
#define SYS_OPEN 8
#define SYS_CLOSE 9
#define SYS_READ 10
#define SYS_WRITE 11

static inline int syscall(int num, int a1, int a2, int a3, int a4, int a5) {
    int ret;

    // Generic system call: pass system call number in eax,
    // up to five parameters in edx, ecx, ebx, edi, and esi.
    // Interrupt kernel with T_SYSCALL.
    //
    // The "volatile" tells the assembler not to optimize this instruction away
    // just because we don't use the return value.
    //
    // The last clause ("cc" and "memory") tells the assembler that
    // this can potentially change the condition codes (such as eflags) and
    // arbitrary memory locations.

    __asm__ volatile("int %1\n"
            : "=a" (ret)
            : "i" (T_SYSCALL),
            "a" (num),
            "d" (a1),
            "c" (a2),
            "b" (a3),
            "D" (a4),
            "S" (a5)
            : "cc", "memory");

    return ret;
}

void sys_cputs(const char *s, int len) {
    syscall(SYS_CPUTS, (int) s, len, 0, 0, 0);
}

void sys_exit(int status) {
    syscall(SYS_EXIT, status, 0, 0, 0, 0);
}

void sys_yield(void) {
    syscall(SYS_YIELD, 0, 0, 0, 0, 0);
}

int sys_get_env_id(void) {
    return syscall(SYS_GET_ENV_ID, 0, 0, 0, 0, 0);
}

int sys_fork(void) {
    return syscall(SYS_FORK, 0, 0, 0, 0, 0);
}

void sys_kill(int pid) {
    syscall(SYS_KILL, pid, 0, 0, 0, 0);
}

void sys_exec(char *pathname) {
    syscall(SYS_EXEC, (int) pathname, 0, 0, 0, 0);
}

int sys_open(char *path, int mode) {
    return syscall(SYS_OPEN, (int) path, mode, 0, 0, 0);
}

int sys_close(int fd) {
    return syscall(SYS_CLOSE, fd, 0, 0, 0, 0);
}

int sys_read(int fd, char *buf, int count) {
    return syscall(SYS_READ, fd, (int) buf, count, 0, 0);
}

int sys_write(int fd, char *buf, int count) {
    return syscall(SYS_WRITE, fd, (int) buf, count, 0, 0);
}
