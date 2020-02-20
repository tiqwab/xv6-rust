// The part of this file comes from jos (lib/entry.S).
// See COPYRIGHT for copyright information.

#define _XV6RUST_ASM
#include <inc/memlayout.h>

// Entrypoint - this is where the kernel (or our parent environment)
// starts us running when we are initially loaded into a new environment.
.text
.globl _start
_start:
	// See if we were started with arguments on the stack
	cmpl $USTACKTOP, %esp
	jne args_exist

	// If not, push dummy argc/argv arguments.
	// This happens when we are loaded by the kernel,
	// because the kernel does not know about passing arguments.
	pushl $0
	pushl $0

args_exist:
	# call libmain
	call umain
    call exit
1:	jmp 1b
