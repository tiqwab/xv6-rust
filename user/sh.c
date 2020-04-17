// The source file is based on sh.c in xv6.
// See COPYRIGHT for copyright information.

#include "user.h"

#define MAXARGS 10

// Parsed command representation
#define EXEC  1
#define REDIR 2
#define PIPE  3
#define LIST  4
#define BACK  5

const char *whitespace = " \t\r\n\v";
const char *symbols = "<|>&; ()";

void exit_err(char *msg) {
    printf("%s\n", msg);
    exit(1);
}

struct cmd {
    int type;
};

struct execcmd {
    int type;
    char *argv[MAXARGS];
    char *eargv[MAXARGS];
    int argc;
};

// Execute cmd. Never returns.
void runcmd(struct cmd *cmd) {
    int p[2];
    struct execcmd *ecmd;

    if (cmd == NULL) exit(0);

    switch (cmd->type) {
        default:
            exit_err("runcmd: illegal type");
            break;
        case EXEC:
            ecmd = (struct execcmd *) cmd;
            if (ecmd->argv[0] == 0) exit(0);
            sys_exec(ecmd->argv[0], ecmd->argv, ecmd->argc);
            printf("exec %s failed\n", ecmd->argv[0]);
            break;
    }

    exit(0);
}

int getcmd(char *buf, int nbuf) {
    int n;
    printf("$ ");
    memset(buf, 0, nbuf);
    n = read(STDIN_FILENO, buf, nbuf);
    if (buf[0] == 0) {
        // EOF
        return -1;
    }
    return n;
}

struct cmd *execcmd(void) {
    // FIXME
    static struct execcmd cmd;
    memset((void *) &cmd, 0, sizeof(struct execcmd));
    cmd.type = EXEC;
    return (struct cmd *) &cmd;
}

int peek(char **ps, char *es, char *toks) {
    char *s;

    s = *ps;
    while (s < es && strchr(whitespace, *s)) s++;
    *ps = s;
    return *s && strchr(toks, *s);
}

int gettoken(char **ps, char *es, char **q, char **eq) {
    char *s;
    int ret;

    s = *ps;
    while (s < es && strchr(whitespace, *s)) s++;
    if (q) *q = s;
    ret = *s;

    switch (*s) {
        case 0:
            break;
        case '|':
        case '(':
        case ')':
        case ';':
        case '&':
        case '<':
            s++;
            break;
        case '>':
            s++;
            if (*s == '>') {
                ret = '+';
                s++;
            }
            break;
        default:
            ret = 'a';
            while (s < es && !strchr(whitespace, *s) && !strchr(symbols, *s)) s++;
            break;
    }

    if (eq) *eq = s;

    while (s < es && strchr(whitespace, *s)) s++;
    *ps = s;
    return ret;
}

struct cmd *parseline(char **, char *);
struct cmd *parseexec(char **, char *);
struct cmd *nulterminate(struct cmd*);

struct cmd *parsecmd(char *s) {
    char *es;
    struct cmd *cmd;

    es = s + strlen(s);
    cmd = parseline(&s, es);
    peek(&s, es, "");
    if (s != es) {
        printf("leftovers: %s\n", s);
        return NULL;
    }
    nulterminate(cmd);
    return cmd;
}

struct cmd *parseline(char **ps, char *es) {
    struct cmd *cmd;

    cmd = parseexec(ps, es);
    return cmd;
}

struct cmd *parseexec(char **ps, char *es) {
    char *q, *eq;
    int tok;
    struct execcmd *cmd;
    struct cmd *ret;

    ret = execcmd();
    cmd = (struct execcmd *) ret;

    while (!peek(ps, es, "|)&;")) {
        if ((tok = gettoken(ps, es, &q, &eq)) == 0) break;
        if (tok != 'a') return NULL;
        cmd->argv[cmd->argc] = q;
        cmd->eargv[cmd->argc] = eq;
        cmd->argc++;
        if (cmd->argc >= MAXARGS) return NULL;
    }

    cmd->argv[cmd->argc] = 0;
    cmd->eargv[cmd->argc] = 0;
    return ret;
}

void umain(int argc, char **argv) {
    static char buf[128];
    int fd, n;

    // Ensure that three file descriptors are open.
    while ((fd = open("console", O_RDWR)) >= 0) {
        if (fd >= 3) {
            close(fd);
            break;
        }
    }

    // Read and run input commands.
    while ((n = getcmd(buf, sizeof(buf))) >= 0) {
        int child = sys_fork();
        if (child < 0) {
            printf("failed to fork\n");
            break;
        } else if (child == 0) {
            // child
            runcmd(parsecmd(buf));
        } else {
            // parent
            while (sys_wait_env_id(child) == 0) {
                __asm__ volatile("pause");
            }
        }
    }
}

// NULL-terminate all the counted strings
struct cmd *nulterminate(struct cmd *cmd) {
    int i;
    struct execcmd *ecmd;

    if (cmd == NULL) return NULL;

    switch (cmd->type) {
        case EXEC:
            ecmd = (struct execcmd *) cmd;
            for (i = 0; ecmd->argv[i]; i++) {
                *ecmd->eargv[i] = 0;
            }
            break;
    }

    return cmd;
}
