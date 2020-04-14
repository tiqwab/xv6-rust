#ifndef _XV6RUST_USER_USER_H
#define _XV6RUST_USER_USER_H

#include "stdarg.h"

#define NULL 0

// for sys_open
#define O_RDONLY 0x000
#define O_WRONLY 0x001
#define O_RDWR   0x002
#define O_CREAT  0x200

typedef int size_t;
typedef unsigned int uintptr_t;

void sys_cputs(const char *s, int len);
void sys_exit(int status);
void sys_yield(void);
int sys_get_env_id(void);
int sys_fork(void);
void sys_kill(int pid);
void sys_exec(char *path);
int sys_open(char *path, int mode);
int sys_close(int fd);

size_t strlen(const char *s);
size_t strnlen(const char *s, size_t maxlen);
void exit(int status);

// stdio
int vcprintf(const char *fmt, va_list ap);
int printf(const char *fmt, ...);
void vprintfmt(void (*putch)(int, void*), void *putdat, const char *fmt, va_list);

#endif /* _XV6RUST_USER_USER_H */
