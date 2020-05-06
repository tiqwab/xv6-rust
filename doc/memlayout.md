### Virtual Memory Layout

The original figure is in inc/memlayout.h in JOS and modified for xv6-rust.

 ```
                                                    Permissions
                                                    kernel/user

    4 Gig -------->  +------------------------------+
                     |                              | RW/--
                     ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
                     :              .               :
                     :              .               :
                     :              .               :
                     |~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~|
                     |                              | RW/--
                     |   Remapped Physical Memory   |
                     |            (*2)              |
                     |                              |
    KERNBASE, ---->  +------------------------------+ 0xf0000000      --+
    KSTACKTOP        |     CPU0's Kernel Stack      | RW/--  KSTKSIZE   |
                     |            (*3)              |                   |
                     | - - - - - - - - - - - - - - -|                   |
                     |      Invalid Memory (*1)     | --/--  KSTKGAP    |
                     +------------------------------+                   |
                     |     CPU1's Kernel Stack      | RW/--  KSTKSIZE   |
                     | - - - - - - - - - - - - - - -|                 PTSIZE
                     |      Invalid Memory (*1)     | --/--  KSTKGAP    |
                     +------------------------------+                   |
                     :              .               :                   |
                     :              .               :                   |
    MMIOLIM ------>  +------------------------------+ 0xefc00000      --+
                     |       Memory-mapped I/O      | RW/--  PTSIZE
                     |            (*4)              |
    MMIOBASE ----->  +------------------------------+ 0xef800000
                     |         Kernel Heap          | RW/-- KHEAP_SIZE = PTSIZE * 3
                     |            (*5)              |
UTOP, KHEAP_BASE ->  +------------------------------+ 0xeec00000
                     |                              |
    USTACKTOP ---->  +------------------------------+ 0xeebfe000
                     |      Normal User Stack       | RW/RW  USTACKSIZE = PGSIZE
                     +------------------------------+ 0xeebfd000
                     |          User Heap           | RW/RW  UHEAPSIZE = PTSIZE * 3
                     |            (*6)              |
    UHEAPBASE ---->  +------------------------------+ 0xedffd000
                     :                              :
                     :                              :
                     |~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~|
                     |     Program Data & Text      |
                     |            (*7)              |
                     +------------------------------+ 0x00800000
                     :                              :
                     :                              :
                     +------------------------------+
                     |  User STAB Data (optional)   |
                     |            (*7)              |
                     +------------------------------+ 0x00200000
                     :                              :
                     :                              :
    0 ------------>  +------------------------------+

 (*1) Note: The kernel ensures that "Invalid Memory" is *never* mapped.
      "Empty Memory" is normally unmapped, but user programs may map pages
      there if desired.

 (*2) The size of Remapped Physical Memory is 256 MB (0xffffffff - 0xf0000000 + 1).
      We might not have so much physical memory, but just set up the mapping anyway.
      (Actual physical memory size would be 128 MB, which comes from the default memoty size setting of QEMU)
      Set up by mem_init() in src/pmap.rs.

 (*3) Kernel's stack is assigned for each CPUs. The design is based on JOS. In xv6, each process has
      kernel stack. The reason why it is OK to assign kernel stack for each CPU, not process is
      "there can be only one JOS environment active in the kernel at a time, so JOS needs only a single
      kernel stack" (ref. [Lab 3: User Environments](https://pdos.csail.mit.edu/6.828/2018/labs/lab3/)).
      (xv6-rust and JOS disables interrupt while executing system call)
      Set up by mem_init_mp() in src/pmap.rs.

 (*4) Memory mapped I/O region. xv6-rust uses it just for lapic.
      We reserve PTSIZE for it, but map only a page for now.
      Set up by lapic_init() in src/lapic.rs and it calls mmio_map_region() in src/pmap.rs.

 (*5) Kernel heap region. We can use alloc libraries in Rust core such as Vector after settings up it.
      Set up by mem_init() in src/pmap.rs

 (*6) Allocated by sbrk.
      The current implementation allows user to have only UHEAPSIZE bytes for heap.

 (*7) Virtual memory layout of user program is based on user/user.ld.
 ```

Virtual Memory Layout after lapic\_init (checked by QEMU monitor):

```
(qemu) info mem
00000000eec00000-00000000ef801000 0000000000c01000 -rw // Kernel Heap and MMIO
00000000eff88000-00000000eff90000 0000000000008000 -rw // CPU 7's kernel stack
00000000eff98000-00000000effa0000 0000000000008000 -rw // CPU 6's kernel stack
00000000effa8000-00000000effb0000 0000000000008000 -rw // CPU 5's kernel stack
00000000effb8000-00000000effc0000 0000000000008000 -rw // CPU 4's kernel stack
00000000effc8000-00000000effd0000 0000000000008000 -rw // CPU 3's kernel stack
00000000effd8000-00000000effe0000 0000000000008000 -rw // CPU 2's kernel stack
00000000effe8000-00000000efff0000 0000000000008000 -rw // CPU 1's kernel stack
00000000efff8000-0000000100000000 0000000010008000 -rw // CPU 0's kernel stack + Remapped Physical Memory
```

Virtual Memory Layout of User Process (checked by QEMU monitor):

```
(qemu) info mem
0000000000200000-0000000000206000 0000000000006000 urw // stab of user program
0000000000800000-0000000000803000 0000000000003000 urw // text and data of user program
00000000eebfd000-00000000eebfe000 0000000000001000 urw // stack for user program
00000000eec00000-00000000ef801000 0000000000c01000 -rw // the below is kernel region.
00000000eff88000-00000000eff90000 0000000000008000 -rw
00000000eff98000-00000000effa0000 0000000000008000 -rw
00000000effa8000-00000000effb0000 0000000000008000 -rw
00000000effb8000-00000000effc0000 0000000000008000 -rw
00000000effc8000-00000000effd0000 0000000000008000 -rw
00000000effd8000-00000000effe0000 0000000000008000 -rw
00000000effe8000-00000000efff0000 0000000000008000 -rw
00000000efff8000-0000000100000000 0000000010008000 -rw
```

### Physical Memory Layout

```
Top phys mem ----->  +------------------------------+ (this might be lower than LAPIC's MMIO region)
                     :                              :
                     :                              :
                     +------------------------------+
                     |       LAPIC's 4K MMIO        | 4 KiB
                     +------------------------------+ 0xfee00000
                     :                              :
                     :                              :
                     ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
                     |      Allocatable Region      |
                     +------------------------------+
                     |          Kernel Heap         | KEHAP_SIZE = PTSIZE * 3
                     |             (*3)             |
                     +------------------------------+
                     |             pages            | npages * sizeof(PageInfo)
                     |             (*3)             |
                     +------------------------------+
                     |     Kernel Page Directory    | 4 KiB
                     |             (*3)             |
    end (*2) ----->  +------------------------------+
                     |      Kernel Data & Text      |
                     +------------------------------+ 0x00100000
                     |             BIOS             |
                     |             (*1)             |
    0 ------------>  +------------------------------+

 (*1) Some part of this regions can be used once kernel code is executed (see PageAllocator::init in src/pmap.rs).
      The detail of BIOS memory layout is: [Memory Map](https://wiki.osdev.org/Memory_Map_(x86))

 (*2) Defined by kernel.ld

 (*3) Assigned by mem_init in src/pmap.rs.
```

Physical memory size (executed in QEMU):

```
# log at the initialization of OS
Physical memory: 131072KB available, base = 640K, extended = 130432K
npages: 32768, npages_basemem: 160
```

