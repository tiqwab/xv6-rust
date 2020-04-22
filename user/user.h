#ifndef _XV6RUST_USER_USER_H
#define _XV6RUST_USER_USER_H

#include "stdarg.h"
#include "stat.h"

#define NULL 0

// for sys_open
#define O_RDONLY 0x000
#define O_WRONLY 0x001
#define O_RDWR   0x002
#define O_CREAT  0x200

// file descriptors
#define STDIN_FILENO 0
#define STDOUT_FILENO 1
#define STDERR_FILENO 2

typedef int size_t;
typedef unsigned int uintptr_t;

void sys_cputs(const char *s, int len);
void sys_exit(int status);
void sys_yield(void);
int sys_get_env_id(void);
int sys_fork(void);
void sys_kill(int pid);
void sys_exec(char *path, char **orig_args, int argc);
int sys_open(char *path, int mode);
int sys_close(int fd);
int sys_read(int fd, char *buf, int count);
int sys_write(int fd, char *buf, int count);
int sys_mknod(char *path, short major, short minor);
int sys_dup(int fd);
int sys_wait_env_id(int pid);
void *sys_sbrk(unsigned int nbytes);
int sys_fstat(int fd, struct stat *statbuf);
char *sys_getcwd(char *buf, unsigned int usize);

size_t strlen(const char *s);
size_t strnlen(const char *s, size_t maxlen);
char *strchr(const char *s, char c);
char *strcpy(char *dest, const char *src);
void exit(int status);
void *memset(void *s, int c, size_t n);
void *memmove(void *dest, const void *src, size_t n);
void *sbrk(unsigned int nbytes);
void *malloc(unsigned int nbytes);
void free(void *ap);
int stat(char *path, struct stat *statbuf);

// stdio
int vcprintf(const char *fmt, va_list ap);
int printf(const char *fmt, ...);
void vprintfmt(void (*putch)(int, void*), void *putdat, const char *fmt, va_list);
int open(char *path, int mode);
int close(int fd);
int read(int fd, char *buf, int count);
int write(int fd, char *buf, int count);

#endif /* _XV6RUST_USER_USER_H */
