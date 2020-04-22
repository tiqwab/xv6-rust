FS_IMG_FILES := \
	$(OBJDIR)/user/hello \
	$(OBJDIR)/user/filetest \
	$(OBJDIR)/user/sh \
	$(OBJDIR)/user/argstest \
	$(OBJDIR)/user/malloctest \
	$(OBJDIR)/user/ls \
	$(OBJDIR)/user/pwd \
	$(OBJDIR)/user/mkdir \

FS_CFLAGS := -Wall -Wextra -MD -I$(TOP)

FS_FORMAT := $(OBJDIR)/fs/fsformat

FS_IMAGE := $(OBJDIR)/fs/fs.img

$(FS_FORMAT): fs/fsformat.c
	@echo + cc -Os $<
	@mkdir -p $(@D)
	$(V)$(CC) $(FS_CFLAGS) -o $@ $<

$(FS_IMAGE): $(FS_FORMAT) $(FS_IMG_FILES)
	@echo + mk $(FS_IMAGE)
	$(V)mkdir -p $(@D)
	$(V)$(FS_FORMAT) $(FS_IMAGE) $(FS_IMG_FILES)
