OBJDIR := obj

# Run 'make V=1' to turn on verbose commands, or 'make V=0' to turn them off.
ifeq ($(V),1)
override V =
endif
ifeq ($(V),0)
override V = @
endif

.PHONY: clean image kernel all test

CC := gcc -pipe
LD := ld
OBJDUMP := objdump
OBJCOPY := objcopy
DD := dd
CP := cp
QEMU := qemu-system-i386
GDB := gdb

TOP := .

# Compiler flags
# -fno-builtin is required to avoid refs to undefined functions in the kernel.
# Only optimize to -O1 to discourage inlining, which complicates backtraces.
CFLAGS := $(CFLAGS) -O1 -fno-builtin -I$(TOP) -MD
CFLAGS += -fno-omit-frame-pointer
CFLAGS += -std=c11
CFLAGS += -static
CFLAGS += -fno-pie
CFLAGS += -Wall -Wextra -Wno-format -Wno-unused -Wno-address-of-packed-member -Werror -gstabs -m32
# -fno-tree-ch prevented gcc from sometimes reordering read_ebp() before
# mon_backtrace()'s function prologue on gcc version: (Debian 4.7.2-5) 4.7.2
CFLAGS += -fno-tree-ch

# Add -fno-stack-protector if the option exists.
CFLAGS += $(shell $(CC) -fno-stack-protector -E -x c /dev/null >/dev/null 2>&1 && echo -fno-stack-protector)

KERN_CFLAGS := $(CFLAGS) -gstabs

# Common linker flags
LDFLAGS := -m elf_i386

# try to generate a unique GDB port
GDBPORT	:= 12345

QEMUOPTS := $(QEMUOPTS)
QEMUOPTS += -drive file=$(OBJDIR)/xv6-rust.img,index=0,media=disk,format=raw -serial mon:stdio -gdb tcp::$(GDBPORT)
QEMUOPTS += $(shell if $(QEMU) -nographic -help | grep -q '^-D '; then echo '-D qemu.log'; fi)


include boot/module.mk

default: all

all: image kernel

.gdbinit: .gdbinit.tmpl
	sed "s/localhost:1234/localhost:$(GDBPORT)/" < $^ > $@

gdb:
	$(GDB) -n -x .gdbinit

# qemu: $(IMAGES) pre-qemu
qemu: image
	$(QEMU) $(QEMUOPTS)

qemu-gdb: image .gdbinit
	$(QEMU) $(QEMUOPTS) -S

test: test-image
	$(QEMU) $(QEMUOPTS)

image: $(OBJDIR)/boot/boot kernel
	$(CP) $(OBJDIR)/boot/boot $(OBJDIR)/xv6-rust.img
	$(DD) conv=notrunc if=target/i686-xv6rust/debug/xv6-rust of=$(OBJDIR)/xv6-rust.img obs=512 seek=1

test-image: $(OBJDIR)/boot/boot kernel
	$(CP) $(OBJDIR)/boot/boot $(OBJDIR)/xv6-rust.img
	dd conv=notrunc if=target/i686-xv6rust/debug/test of=$(OBJDIR)/xv6-rust.img obs=512 seek=1

kernel:
	cargo xbuild --target i686-xv6rust.json

clean:
	rm -rf $(OBJDIR)
