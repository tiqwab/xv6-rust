#define NULL 0

typedef int size_t;

void sys_cputs(const char *s, int len);
void sys_exit(int status);

// TODO: enable to format string
int printf(const char *s, ...);
size_t strlen(const char *s);
void exit(int status);
