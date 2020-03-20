// This file based on `fs.h` and `stat.h` in xv6.
// See COPYRIGHT for copyright information.

/* from `fs.h` */

#ifndef _XV6RUST_FS_H
#define _XV6RUST_FS_H

#define ROOTINO 1 // root i-number
#define BLKSIZE 512
#define FSSIZE 1000 // size of file system in blocks

#define MAXOPBLOCKS 10  // max # of blocks any FS op writes
#define LOGSIZE (MAXOPBLOCKS*3) // max data blocks in on-disk log
#define NBUF (MAXOPBLOCKS*3) // size of disk block cache

// Block containing inode i
#define IBLOCK(i, sb) ((i) / IPB + sb.inodestart)

// Inodes per block.
#define IPB (BLKSIZE / sizeof(struct dinode))

// Disk layout:
// [ boot block | super block | log | inode blocks |
//                                          free bit map | data blocks]
//
// mkfs computes the super block and builds an initial file system. The
// super block describes the disk layout:
struct superblock {
    uint size;         // Size of file system image (blocks)
    uint nblocks;      // Number of data blocks
    uint ninodes;      // Number of inodes.
    uint nlog;         // Number of log blocks
    uint logstart;     // Block number of first log block
    uint inodestart;   // Block number of first inode block
    uint bmapstart;    // Block number of first free map block
};

#define NDIRECT 12
#define NINDIRECT (BLKSIZE / sizeof(uint))
#define MAXFILE (NDIRECT + NINDIRECT)

// On-disk inode structure
struct dinode {
    short type; // File type
    short major; // Major device number (T_DEV only)
    short minor; // Minor device number (T_DEV only)
    short nlink; // Number of links to inode in file system
    uint size; // Size of file (bytes)
    uint addrs[NDIRECT+1]; // Data block addresses
};

// Directory is a file containing a sequence of dirent structures.
#define DIRSIZ 14

struct dirent {
    ushort inum;
    char name[DIRSIZ];
};

/* from `stat.h` */

#define T_DIR  1   // Directory
#define T_FILE 2   // File
#define T_DEV  3   // Device

struct stat {
  short type; // Type of file
  int dev; // File system's disk device
  uint ino; // Inode number
  short nlink; // Number of links to file
  uint size; // Size of file in bytes
};

#endif /* _XV6RUST_FS_H */
