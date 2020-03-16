// This file based on `inc/fs.h` in JOS.
// See COPYRIGHT for copyright information.

#ifndef _XV6RUST_FS_H
#define _XV6RUST_FS_H

#include "mmu.h"
// #include "types.h"

// Bytes per file system block - same as page size
#define BLKSIZE PGSIZE
#define BLKBITSIZE (BLKSIZE * 8)

// Maximum size of a filename (a single path component), including null
// Must be a multiple of 4
#define MAXNAMELEN 128

// Number of block pointers in a File descriptor
#define NDIRECT 10
// Number of direct block pointers in an indirect block
#define NINDIRECT (BLKSIZE / 4)

#define MAXFILESIZE ((NDIRECT + NINDIRECT) * BLKSIZE)

// File types
#define FTYPE_REG 0 // Regular file
#define FTYPE_DIR 1 // Directory

struct File {
    char f_name[MAXNAMELEN]; // filename
    off_t f_size; // file size in bytes
    uint32_t f_type; // file type

    // Block pointers.
    // A block is allocated iff its value is != 0.
    uint32_t f_direct[NDIRECT]; // direct blocks
    uint32_t f_indirect; // indirect block

    // Pad out to 256 bytes; must do arithmetic in case we're compiling
    // fsformat on a 64-bit machine.
    uint8_t f_pad[256 - MAXNAMELEN - 8 - 4*NDIRECT - 4];
} __attribute__((packed));    // required only on some 64-bit machines

// File system super-block (both in-memory and on-disk)

#define FS_MAGIC 0x4A0530AE // related vaguely to 'J\0S!'

struct Super {
    uint32_t s_magic; // Magic number: FS_MAGIC
    uint32_t s_nblocks; // Total number of blocks on disk
    struct File s_root; // Root directory node
};

#endif /* _XV6RUST_FS_H */
