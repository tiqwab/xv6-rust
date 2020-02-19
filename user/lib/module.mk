USER_LIB_ARCHIVE := $(OBJDIR)/user/lib/libxv6rust.a

USER_LIB_CFLAGS := $(CFLAGS) -gstabs

USER_LIB_SRCS := \
	user/lib/printf.c \
	user/lib/strlen.c \
	user/lib/syscall.c \

USER_LIB_OBJS := $(patsubst user/lib/%.c, $(OBJDIR)/user/lib/%.o, $(USER_LIB_SRCS))

$(OBJDIR)/user/lib/%.o: user/lib/%.c
	@echo + cc -Os $<
	@mkdir -p $(@D)
	$(V)$(CC) -nostdinc $(USER_LIB_CFLAGS) -c -o $@ $<

$(USER_LIB_ARCHIVE): $(USER_LIB_OBJS)
	@echo + ar $@
	$(V)$(AR) r $@ $(USER_LIB_OBJS)
