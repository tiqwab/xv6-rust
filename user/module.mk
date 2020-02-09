UPROGS += \
	$(OBJDIR)/user/nop

UENTRYOBJ := $(OBJDIR)/user/entry.o

$(UENTRYOBJ): user/entry.S
	@echo + cc -Os $<
	@mkdir -p $(@D)
	$(V)$(CC) -nostdinc $(USER_CFLAGS) -c -o $@ $<

$(OBJDIR)/user/%.o: user/%.c
	@echo + cc -Os $<
	@mkdir -p $(@D)
	$(V)$(CC) -nostdinc $(USER_CFLAGS) -c -o $@ $<

$(OBJDIR)/user/%: $(OBJDIR)/user/%.o $(UENTRYOBJ) user/user.ld
	@echo + ld $@
	# $(V)$(LD) -o $@ -T user/user.ld $(LDFLAGS) -nostdlib $(OBJDIR)/lib/entry.o $@.o -L$(OBJDIR)/lib $(USERLIBS:%=-l%) $(GCC_LIB)
	$(V)$(LD) -o $@ -T user/user.ld $(LDFLAGS) -nostdlib $< $(UENTRYOBJ)
	$(V)$(OBJDUMP) -S $@ > $@.asm
	$(V)$(NM) -n $@ > $@.sym
