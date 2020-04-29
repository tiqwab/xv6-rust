UPROGS += \
	$(OBJDIR)/user/nop \
	$(OBJDIR)/user/hello \
	$(OBJDIR)/user/yield \
	$(OBJDIR)/user/forktest \
	$(OBJDIR)/user/spin \
	$(OBJDIR)/user/init \
	$(OBJDIR)/user/filetest \
	$(OBJDIR)/user/sh \
	$(OBJDIR)/user/argstest \
	$(OBJDIR)/user/malloctest \
	$(OBJDIR)/user/ls \
	$(OBJDIR)/user/pwd \
	$(OBJDIR)/user/mkdir \
	$(OBJDIR)/user/echo \
	$(OBJDIR)/user/whello \
	$(OBJDIR)/user/cat \
	$(OBJDIR)/user/pipetest \
	$(OBJDIR)/user/wc \

include user/lib/module.mk

USER_CFLAGS := $(CFLAGS) -gstabs
USER_GCC_LIB := $(shell $(CC) $(CFLAGS) -print-libgcc-file-name)
UENTRYOBJ := $(OBJDIR)/user/entry.o

$(UENTRYOBJ): user/entry.S
	@echo + cc -Os $<
	@mkdir -p $(@D)
	$(V)$(CC) -nostdinc $(USER_CFLAGS) -c -o $@ $<

$(OBJDIR)/user/%.o: user/%.c
	@echo + cc -Os $<
	@mkdir -p $(@D)
	$(V)$(CC) -nostdinc $(USER_CFLAGS) -c -o $@ $<

$(OBJDIR)/user/%: $(OBJDIR)/user/%.o $(USER_LIB_ARCHIVE) $(UENTRYOBJ) user/user.ld
	@echo + ld $@
	# $(V)$(LD) -o $@ -T user/user.ld $(LDFLAGS) -nostdlib $(OBJDIR)/lib/entry.o $@.o -L$(OBJDIR)/lib $(USERLIBS:%=-l%) $(GCC_LIB)
	$(V)$(LD) -o $@ -T user/user.ld $(LDFLAGS) -nostdlib $< $(UENTRYOBJ) $(USER_LIB_ARCHIVE) $(USER_GCC_LIB)
	$(V)$(OBJDUMP) -S $@ > $@.asm
	$(V)$(NM) -n $@ > $@.sym
