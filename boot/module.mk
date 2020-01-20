BOOT_OBJS := $(OBJDIR)/boot/boot.o $(OBJDIR)/boot/main.o

$(OBJDIR)/boot/%.o: boot/%.c
	@echo + cc -Os $<
	@mkdir -p $(@D)
	$(V)$(CC) -nostdinc $(KERN_CFLAGS) -Os -c -o $@ $<

$(OBJDIR)/boot/%.o: boot/%.S
	@echo + as $<
	@mkdir -p $(@D)
	$(V)$(CC) -nostdinc $(KERN_CFLAGS) -c -o $@ $<

$(OBJDIR)/boot/base.img: boot/base.img
	@echo + cp $<

# boot/base.img is 512 bytes empty file except for last 0x55 0xAA. This is MBR format.
$(OBJDIR)/boot/boot: $(BOOT_OBJS) boot/base.img
	@echo + ld boot/boot
	$(V)$(LD) $(LDFLAGS) -e start -Ttext 0x7C00 -o $@.out $(BOOT_OBJS)
	$(V)$(OBJDUMP) -S $@.out >$@.asm
	$(V)$(OBJCOPY) -S -O binary -j .text $@.out $@.bin
	$(V)$(CP) boot/base.img $@
	$(V)$(DD) conv=notrunc if=$@.bin of=$@
