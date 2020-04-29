#include "user.h"

#define BUF_LEN 128

const char *whitespace = " \t\r\n\v";

struct counter {
    int line;
    int word;
    int byte;
};

// Emit line count, word count, byte count to stdout
void wc(struct counter *ct, const int fd) {
    int line_count = 0, word_count = 0, byte_count = 0;
    int n;
    char buf[BUF_LEN];
    int was_whitespace = 1;

    while ((n = read(fd, buf, BUF_LEN)) > 0) {
        buf[n] = '\0';
        for (int i = 0; i < n; i++) {
            if (buf[i] == '\n') {
                line_count++;
            }

            int is_whitespace = strchr(whitespace, buf[i]) != NULL;
            if (was_whitespace && !is_whitespace) {
                word_count++;
            }
            was_whitespace = is_whitespace;

            byte_count++;
        }
    }

    ct->line = line_count;
    ct->word = word_count;
    ct->byte = byte_count;
}

void umain(int argc, char **argv) {
    struct counter ct;
    ct.line = 0;
    ct.word = 0;
    ct.byte = 0;

    if (argc < 2) {
        wc(&ct, STDIN_FILENO);
    } else {
        for (int i = 1; i < argc; i++) {
            int fd;
            char *path = argv[i];
            if ((fd = open(path, O_RDONLY)) < 0) {
                printf("wc: cannot open %s\n", path);
                return;
            }
            wc(&ct, fd);
            close(fd);
        }
    }

    printf("%d %d %d\n", ct.line, ct.word, ct.byte);
}
