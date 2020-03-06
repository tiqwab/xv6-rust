#define NULL 0

typedef int size_t;

void sys_cputs(const char *s, int len);
void sys_exit(int status);
void sys_yield(void);
int sys_get_env_id(void);

// TODO: enable to format string
int printf(const char *s, ...);
size_t strlen(const char *s);
void exit(int status);
