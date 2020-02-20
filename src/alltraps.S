# This file is based on kern/trapentry.S in JOS.
# See COPYRIGHT for copyright information.

#define _XV6RUST_ASM
#include <memlayout.h>

.global _alltraps
.align 4;
_alltraps:
  # store registers
  pushl %ds
  pushl %es
  pushal

  # load GD_KD into %ds and %es 
  mov $GD_KD,%ax
  movw %ax,%ds
  movw %ax,%es

  # pushl %esp to pass a pointer to the Trapframe as argument to trap
  pushl %esp
  call trap
