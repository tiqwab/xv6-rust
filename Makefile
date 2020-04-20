OBJDIR := obj
KERNDIR := src

# Run 'make V=1' to turn on verbose commands, or 'make V=0' to turn them off.
ifeq ($(V),1)
override V =
endif
ifeq ($(V),0)
override V = @
endif

.PHONY: clean image kernel kernel-asm all test

CC := gcc -pipe
LD := ld
OBJDUMP := objdump
OBJCOPY := objcopy
DD := dd
CP := cp
NM := nm
AR := ar
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

BOOT_CFLAGS := $(CFLAGS) -gstabs

# Common linker flags
LDFLAGS := -m elf_i386

# try to generate a unique GDB port
GDBPORT	:= 12345

CPUS ?= 1

UPROGS :=

include boot/module.mk
include user/module.mk
include fs/module.mk

# Enter QEMU monitor by 'Ctrl+a then c' if -serial mon:stdio is specified
# ref. https://kashyapc.wordpress.com/2016/02/11/qemu-command-line-behavior-of-serial-stdio-vs-serial-monstdio/
QEMUOPTS := $(QEMUOPTS)
QEMUOPTS += -drive file=$(OBJDIR)/xv6-rust.img,index=0,media=disk,format=raw -serial mon:stdio -gdb tcp::$(GDBPORT) -smp $(CPUS)
QEMUOPTS += -drive file=$(FS_IMAGE),index=1,media=disk,format=raw
QEMUOPTS += $(shell if $(QEMU) -nographic -help | grep -q '^-D '; then echo '-D qemu.log'; fi)

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

KERN_BINARY := target/i686-xv6rust/debug/xv6-rust
KERN_TEST_BINARY := target/i686-xv6rust/debug/test

image: $(OBJDIR)/boot/boot kernel $(FS_IMAGE)
	$(CP) $(OBJDIR)/boot/boot $(OBJDIR)/xv6-rust.img
	$(DD) conv=notrunc if=$(KERN_BINARY) of=$(OBJDIR)/xv6-rust.img obs=512 seek=1

test-image: $(OBJDIR)/boot/boot kernel $(FS_IMAGE)
	$(CP) $(OBJDIR)/boot/boot $(OBJDIR)/xv6-rust.img
	dd conv=notrunc if=$(KERN_TEST_BINARY) of=$(OBJDIR)/xv6-rust.img obs=512 seek=1

$(KERNDIR)/vectors.S: vectors.sh
	./vectors.sh > $@

# `-C link-arg` option can be passed by target json (such as post-link-args field) instead?
KERN_BINARY_ARGS := $(patsubst %,-C link-arg=%, $(UPROGS))
KERN_RUSTFLAGS := -Z print-link-args -C link-arg=-b -C link-arg=binary $(KERN_BINARY_ARGS) -C link-arg=-b -C link-arg=default

# KERN_CFLAGS is currently used only for compiling c program by cc crate.
# '--compress-debug-sections' is temporary fix for 'contains a compressed section, but zlib is not available'
KERN_CFLAGS := -Wa,--compress-debug-sections=none -Wl,--compress-debug-sections=none

KERN_CARGOFLAGS :=
ifdef DEBUG
	KERN_CARGOFLAGS += --features debug
endif

kernel: $(UPROGS) $(KERNDIR)/vectors.S
	@mkdir -p $(OBJDIR)
	RUSTFLAGS="$(KERN_RUSTFLAGS)" CFLAGS="$(KERN_CFLAGS)" cargo xbuild --target i686-xv6rust.json --verbose $(KERN_CARGOFLAGS)
	$(OBJDUMP) -S $(KERN_BINARY) > $(OBJDIR)/xv6-rust.asm
	$(OBJDUMP) -S $(KERN_TEST_BINARY) > $(OBJDIR)/xv6-rust-test.asm

clean:
	rm -rf $(OBJDIR)
	cargo clean
