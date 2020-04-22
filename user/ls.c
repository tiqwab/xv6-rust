#include "user.h"

char *fmtname(char *path) {
    static char buf[DIR_SIZ + 1];
    char *p;

    // Find first character after last slash.
    for (p = path + strlen(path); p >= path && *p != '/'; p--) {}
    p++;

    // Return blank-padded name.
    if (strlen(p) >= DIR_SIZ) {
        return p;
    }
    memmove(buf, p, strlen(p));
    memset(buf + strlen(p), ' ', DIR_SIZ - strlen(p));
    return buf;
}

void ls(char *path) {
    char buf[512], *p;
    int fd;
    unsigned int parent_sz, sz;
    struct dirent de;
    struct stat st;

    if ((fd = open(path, O_RDONLY)) < 0) {
        printf("ls: cannot open %s\n", path);
        return;
    }

    if (sys_fstat(fd, &st) < 0) {
        printf("ls: cannot stat %s\n", path);
        close(fd);
        return;
    }
    parent_sz = st.size;

    switch (st.typ) {
        case T_FILE:
            printf("%s %d %d %d\n", fmtname(path), st.typ, st.inum, st.size);
            break;
        case T_DIR:
            if (strlen(path) + 1 + DIR_SIZ + 1 > (int) sizeof(buf)) {
                printf("ls: path too long\n");
                break;
            }
            strcpy(buf, path);
            p = buf + strlen(buf);
            *p++ = '/';

            sz = 0;
            while (sz < parent_sz) {
                read(fd, (char *) &de, sizeof(de));
                sz += sizeof(de);
                if (de.inum == 0) {
                    continue;
                }
                memmove(p, de.name, DIR_SIZ);
                p[DIR_SIZ] = 0;
                if (stat(buf, &st) < 0) {
                    printf("ls: cannot stat %s\n", buf);
                    continue;
                }
                printf("%s %d %d %d\n", fmtname(buf), st.typ, st.inum, st.size);
            }
            break;
    }

    close(fd);
}

void umain(int argc, char **argv) {
    if (argc < 2) {
        ls(".");
    } else {
        for (int i = 1; i < argc; i++) {
            ls(argv[i]);
        }
    }
}
