FS_IMG_FILES :=

FS_CFLAGS := -Wall -Wextra -MD -I$(TOP)

FS_FORMAT := $(OBJDIR)/fs/fsformat

FS_IMAGE := $(OBJDIR)/fs/fs.img

$(FS_FORMAT): fs/fsformat.c
	@echo + cc -Os $<
	@mkdir -p $(@D)
	$(V)$(CC) $(FS_CFLAGS) -o $@ $<

$(FS_IMAGE): $(FS_FORMAT)
	@echo + mk $(FS_IMAGE)
	$(V)mkdir -p $(@D)
	$(V)$(FS_FORMAT) $(FS_IMAGE) 1024 $(FS_IMG_FILES)
