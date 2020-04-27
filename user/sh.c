// The source file is based on sh.c in xv6.
// See COPYRIGHT for copyright information.

#include "user.h"

#define MAXARGS 10
#define BUF_LEN 128

// Parsed command representation
#define EXEC  1
#define REDIR 2
#define PIPE  3
#define LIST  4
#define BACK  5

const char *whitespace = " \t\r\n\v";
const char *symbols = "<|>&; ()";

void exit_err(char *msg) {
    printf("sh: %s\n", msg);
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

struct redircmd {
    int type;
    struct cmd *cmd;
    char *file;
    char *efile;
    int mode;
    int fd;
};

struct pipecmd {
    int type;
    struct cmd *left;
    struct cmd *right;
};

// Execute cmd. Never returns.
void runcmd(struct cmd *cmd) {
    int p[2];
    struct execcmd *ecmd;
    struct redircmd *rcmd;
    struct pipecmd *pcmd;

    if (cmd == NULL) exit(0);

    switch (cmd->type) {
        default:
            exit_err("cmd illegal type");
            break;
        case EXEC:
            ecmd = (struct execcmd *) cmd;
            if (ecmd->argv[0] == 0) exit(0);
            sys_exec(ecmd->argv[0], ecmd->argv, ecmd->argc);
            printf("exec %s failed\n", ecmd->argv[0]);
            break;
        case REDIR:
            rcmd = (struct redircmd *) cmd;
            close(rcmd->fd);
            if (open(rcmd->file, rcmd->mode) < 0) {
                printf("open %s failed\n", rcmd->file);
            } else {
                runcmd(rcmd->cmd);
            }
            break;
        case PIPE:
            pcmd = (struct pipecmd *) cmd;

            if (sys_pipe(p) < 0) {
                exit_err("pipe failed");
            }

            int left_id;
            if ((left_id = sys_fork()) < 0) {
                exit_err("fork failed");
            } else if (left_id == 0) {
                close(STDOUT_FILENO);
                sys_dup(p[1]);
                close(p[0]);
                close(p[1]);
                runcmd(pcmd->left);
            }

            int right_id;
            if ((right_id = sys_fork()) < 0) {
                exit_err("fork failed");
            } else if (right_id == 0) {
                close(STDIN_FILENO);
                sys_dup(p[0]);
                close(p[0]);
                close(p[1]);
                runcmd(pcmd->right);
            }

            close(p[0]);
            close(p[1]);
            wait_env_id(left_id);
            wait_env_id(right_id);
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
    struct execcmd *cmd;

    cmd = malloc(sizeof(*cmd));
    memset((void *) cmd, 0, sizeof(*cmd));
    cmd->type = EXEC;
    return (struct cmd *) cmd;
}

struct cmd *redircmd(struct cmd *subcmd, char *file, char *efile, int mode, int fd) {
    struct redircmd *cmd;

    cmd = malloc(sizeof(*cmd));
    memset((void *) cmd, 0, sizeof(*cmd));
    cmd->type = REDIR;
    cmd->cmd = subcmd;
    cmd->file = file;
    cmd->efile = efile;
    cmd->mode = mode;
    cmd->fd = fd;
    return (struct cmd *) cmd;
}

struct cmd *pipecmd(struct cmd *left, struct cmd *right) {
    struct pipecmd *cmd;

    cmd = malloc(sizeof(*cmd));
    memset(cmd, 0, sizeof(*cmd));
    cmd->type = PIPE;
    cmd->left = left;
    cmd->right = right;
    return (struct cmd *) cmd;
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
    if (q) {
        *q = s;
    }
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
struct cmd *parsepipe(char **ps, char *es);
struct cmd *parseexec(char **, char *);
struct cmd *parseredirs(struct cmd *cmd, char **ps, char *es);
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

    cmd = parsepipe(ps, es);

    // for backcmd

    // for listcmd

    return cmd;
}

struct cmd *parsepipe(char **ps, char *es) {
    struct cmd *cmd;

    cmd = parseexec(ps, es);
    if (peek(ps, es, "|")) {
        gettoken(ps, es, 0, 0);
        cmd = pipecmd(cmd, parsepipe(ps, es));
    }
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
        ret = parseredirs(ret, ps, es);
    }

    cmd->argv[cmd->argc] = 0;
    cmd->eargv[cmd->argc] = 0;
    return ret;
}

struct cmd *parseredirs(struct cmd *cmd, char **ps, char *es) {
    char *q, *eq;
    int tok;

    while (peek(ps, es, "<>")) {
        tok = gettoken(ps, es, 0, 0);
        if (gettoken(ps, es, &q, &eq) != 'a') {
            printf("missing file for redirection");
            return NULL;
        }

        switch(tok) {
            case '<':
                cmd = redircmd(cmd, q, eq, O_RDONLY, 0);
                break;
            case '>':
                cmd = redircmd(cmd, q, eq, O_WRONLY | O_CREAT, 1);
                break;
            case '+':
                cmd = redircmd(cmd, q, eq, O_WRONLY | O_CREAT, 1);
                break;
        }
    }

    return cmd;
}

void umain(int argc, char **argv) {
    static char buf[BUF_LEN];
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
        buf[strlen(buf) - 1] = 0; // chop \n

        if (buf[0] == 'c' && buf[1] == 'd' && buf[2] == ' ') {
            // Chdir must be called by the parent, not the child.
            if (sys_chdir(buf + 3) != 0) {
                printf("cd: cannot cd %s\n", buf + 3);
            }
        } else {
            // check cmd existence
            char cmd[BUF_LEN];
            memmove(cmd, buf, BUF_LEN);
            char *cmd_end = strchr(cmd, ' ');
            if (cmd_end != NULL) {
                *cmd_end = '\0';
            }

            if (cmd[0] != '/' && strchr(cmd, '/') == NULL) {
                int fd;
                if ((fd = open(cmd, O_RDONLY)) < 0) {
                    // prepend '/' to path
                    int len = strnlen(buf, BUF_LEN);
                    if (len < BUF_LEN - 2) {
                        for (int i = len + 1; i > 0; i--) {
                            buf[i] = buf[i - 1];
                        }
                        buf[0] = '/';
                    } else {
                        printf("sh: command not found: %s\n", buf);
                        break;
                    }
                } else {
                    close(fd);
                }
            }

            int child = sys_fork();
            if (child < 0) {
                printf("sh: fork failed\n");
                break;
            } else if (child == 0) {
                // child
                runcmd(parsecmd(buf));
            } else {
                // parent
                wait_env_id(child);
            }
        }
    }
}

// NULL-terminate all the counted strings
struct cmd *nulterminate(struct cmd *cmd) {
    int i;
    struct execcmd *ecmd;
    struct redircmd *rcmd;
    struct pipecmd *pcmd;

    if (cmd == NULL) return NULL;

    switch (cmd->type) {
        case EXEC:
            ecmd = (struct execcmd *) cmd;
            for (i = 0; ecmd->argv[i]; i++) {
                *ecmd->eargv[i] = 0;
            }
            break;
        case REDIR:
            rcmd = (struct redircmd *) cmd;
            nulterminate(rcmd->cmd);
            *rcmd->efile = 0;
            break;
        case PIPE:
            pcmd = (struct pipecmd *) cmd;
            nulterminate(pcmd->left);
            nulterminate(pcmd->right);
            break;
    }

    return cmd;
}
