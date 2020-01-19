#ifndef _XV6RUST_BOOT_H
#define _XV6RUST_BOOT_H

/*
 * from inc/types.h in jos
 */

#ifndef _XV6RUST_ASM

// Explicitly-sized versions of integer types
typedef __signed char int8_t;
typedef unsigned char uint8_t;
typedef short int16_t;
typedef unsigned short uint16_t;
typedef int int32_t;
typedef unsigned int uint32_t;
typedef long long int64_t;
typedef unsigned long long uint64_t;

#endif /* _XV6RUST_ASM */

/*
 * from inc/mem.h in jos
 */

// Macros to build GDT entries in assembly.
// originally included in <inc/mmu.h>.
#define SEG_NULL \
  .word 0, 0; \
  .byte 0, 0, 0, 0
#define SEG(type,base,lim) \
  .word (((lim) >> 12) & 0xffff), ((base) & 0xffff); \
  .byte (((base) >> 16) & 0xff), (0x90 | (type)), \
    (0xC0 | (((lim) >> 28) & 0xf)), (((base) >> 24) & 0xff)
#define STA_X     0x8       // Executable segment
#define STA_W     0x2       // Writeable (non-executable segments)
#define STA_R     0x2       // Readable (executable segments)

/*
 * from inc/x86.h in jos
 */

#ifndef _XV6RUST_ASM
static inline uint8_t inb(int port) {
    uint8_t data;
    __asm__ volatile("inb %w1,%0" : "=a" (data) : "d" (port));
    return data;
}

static inline void insb(int port, void *addr, int cnt) {
    __asm__ volatile("cld\n\trepne\n\tinsb"
            : "=D" (addr), "=c" (cnt)
            : "d" (port), "0" (addr), "1" (cnt)
            : "memory", "cc");
}

static inline uint16_t inw(int port) {
    uint16_t data;
    __asm__ volatile("inw %w1,%0" : "=a" (data) : "d" (port));
    return data;
}

static inline void insl(int port, void *addr, int cnt) {
    __asm__ volatile("cld\n\trepne\n\tinsl"
            : "=D" (addr), "=c" (cnt)
            : "d" (port), "0" (addr), "1" (cnt)
            : "memory", "cc");
}

static inline void outb(int port, uint8_t data) {
    __asm__ volatile("outb %0,%w1" : : "a" (data), "d" (port));
}

static inline void outsb(int port, const void *addr, int cnt) {
    __asm__ volatile("cld\n\trepne\n\toutsb"
            : "=S" (addr), "=c" (cnt)
            : "d" (port), "0" (addr), "1" (cnt)
            : "cc");
}

static inline void outw(int port, uint16_t data) {
    __asm__ volatile("outw %0,%w1" : : "a" (data), "d" (port));
}

static inline void outsl(int port, const void *addr, int cnt) {
    __asm__ volatile("cld\n\trepne\n\toutsl"
            : "=S" (addr), "=c" (cnt)
            : "d" (port), "0" (addr), "1" (cnt)
            : "cc");
}

#endif /* _XV6RUST_ASM */

/*
 * from inc/elf.h in jos
 */

#ifndef _XV6RUST_ASM

#define ELF_MAGIC 0x464C457FU	/* "\x7FELF" in little endian */

struct Elf {
	uint32_t e_magic;	// must equal ELF_MAGIC
	uint8_t e_elf[12];
	uint16_t e_type;
	uint16_t e_machine;
	uint32_t e_version;
	uint32_t e_entry;
	uint32_t e_phoff;
	uint32_t e_shoff;
	uint32_t e_flags;
	uint16_t e_ehsize;
	uint16_t e_phentsize;
	uint16_t e_phnum;
	uint16_t e_shentsize;
	uint16_t e_shnum;
	uint16_t e_shstrndx;
};

struct Proghdr {
	uint32_t p_type;
	uint32_t p_offset;
	uint32_t p_va;
	uint32_t p_pa;
	uint32_t p_filesz;
	uint32_t p_memsz;
	uint32_t p_flags;
	uint32_t p_align;
};

struct Secthdr {
	uint32_t sh_name;
	uint32_t sh_type;
	uint32_t sh_flags;
	uint32_t sh_addr;
	uint32_t sh_offset;
	uint32_t sh_size;
	uint32_t sh_link;
	uint32_t sh_info;
	uint32_t sh_addralign;
	uint32_t sh_entsize;
};

// Values for Proghdr::p_type
#define ELF_PROG_LOAD		1

// Flag bits for Proghdr::p_flags
#define ELF_PROG_FLAG_EXEC	1
#define ELF_PROG_FLAG_WRITE	2
#define ELF_PROG_FLAG_READ	4

// Values for Secthdr::sh_type
#define ELF_SHT_NULL		0
#define ELF_SHT_PROGBITS	1
#define ELF_SHT_SYMTAB		2
#define ELF_SHT_STRTAB		3

// Values for Secthdr::sh_name
#define ELF_SHN_UNDEF		0

#endif /* _XV6RUST_ASM */

#endif /* _XV6RUST_BOOT_H */
