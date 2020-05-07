### Disk layout

xv6-rust uses obj/fs/fs.img as a storage disk. Sector size is 512 bytes.

See [xv6-book](https://pdos.csail.mit.edu/6.828/2018/xv6/book-rev10.pdf) to know the detail of each sector.

- sector 0: boot
  - but this sector is not used because the boot disk is obj/xv6-rust.img not obj/fs/fs.img.
- sector 1: superblock
  - see src/superblock.rs
- sector 2-31: log
  - size is determined by LOGSIZE in inc/fs.h
- sector 32-57: inode
  - size is calculated from NINODES in fs/fsformat.c and size of inode
- sector 58 : bit map
  - size is calculated from FSSIZE in inc/fs.h
- sector 59- : data

Actual image example is (checked by od):

```
# boot starts from 0x000000, but is empty.
000000 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00  >................<
*

# superblock starts from 0x000200.
000200 e8 03 00 00 ad 03 00 00 c8 00 00 00 1e 00 00 00  >................<
000210 02 00 00 00 20 00 00 00 3a 00 00 00 00 00 00 00  >.... ...:.......<
000220 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00  >................<
*

# log starts from 0x000400.
000400 00 00 00 00 21 00 00 00 3b 00 00 00 00 00 00 00  >....!...;.......<
000410 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00  >................<
*
...

# inodes starts from 0x004000, but inode zero is empty. See DInode in src/fs.rs for inode structure.
# Inode 1 is for the root directory. The first block is 0x3b.
*
004040 01 00 00 00 00 00 01 00 00 02 00 00 3b 00 00 00  >............;...<
004050 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00  >................<
*
...

# bit map starts from 0x007400.
007400 ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff  >................<
*
007450 ff ff ff ff ff ff ff ff ff ff ff 00 00 00 00 00  >................<
007460 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00  >................<
*
...

# data starts from 0x007600.
# This is block 0x3b (the first block of inode 1)
007600 01 00 00 00 2e 00 00 00 00 00 00 00 00 00 00 00  >................<
007610 01 00 00 00 2e 2e 00 00 00 00 00 00 00 00 00 00  >................<
007620 02 00 00 00 68 65 6c 6c 6f 00 00 00 00 00 00 00  >....hello.......<
007630 03 00 00 00 66 69 6c 65 74 65 73 74 00 00 00 00  >....filetest....<
007640 04 00 00 00 73 68 00 00 00 00 00 00 00 00 00 00  >....sh..........<
007650 05 00 00 00 61 72 67 73 74 65 73 74 00 00 00 00  >....argstest....<
007660 06 00 00 00 6d 61 6c 6c 6f 63 74 65 73 74 00 00  >....malloctest..<
007670 07 00 00 00 6c 73 00 00 00 00 00 00 00 00 00 00  >....ls..........<
007680 08 00 00 00 70 77 64 00 00 00 00 00 00 00 00 00  >....pwd.........<
007690 09 00 00 00 6d 6b 64 69 72 00 00 00 00 00 00 00  >....mkdir.......<
0076a0 0a 00 00 00 65 63 68 6f 00 00 00 00 00 00 00 00  >....echo........<
0076b0 0b 00 00 00 77 68 65 6c 6c 6f 00 00 00 00 00 00  >....whello......<
0076c0 0c 00 00 00 63 61 74 00 00 00 00 00 00 00 00 00  >....cat.........<
0076d0 0d 00 00 00 70 69 70 65 74 65 73 74 00 00 00 00  >....pipetest....<
0076e0 0e 00 00 00 77 63 00 00 00 00 00 00 00 00 00 00  >....wc..........<
0076f0 0f 00 00 00 63 6f 6e 73 6f 6c 65 00 00 00 00 00  >....console.....<
007700 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00  >................<
*
...

```
