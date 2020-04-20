#define DIR_SIZ 12

#define T_DIR 1
#define T_FILE 2

// FIXME: the same definition is in src/fs.rs
struct dirent {
    unsigned int inum;
    char name[DIR_SIZ];
};

// FIXME: the same definition is in src/fs.rs
struct stat {
    unsigned short typ;
    unsigned int dev;
    unsigned int inum;
    unsigned short nlink;
    unsigned int size;
};
