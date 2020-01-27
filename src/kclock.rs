use crate::x86;

// ref. https://wiki.osdev.org/CMOS

#[allow(dead_code)]
const IO_RTC: u16 = 0x70;

// start of NVRAM: offset 14
#[allow(dead_code)]
const MC_NVRAM_START: u8 = 0xe;

// base memory size
#[allow(dead_code)]
pub(crate) const NVRAM_BASELO: u8 = MC_NVRAM_START + 7;
#[allow(dead_code)]
pub(crate) const NVRAM_BASEHI: u8 = MC_NVRAM_START + 8;

// extended memory size (between 1MB and 16MB)
#[allow(dead_code)]
pub(crate) const NVRAM_EXTLO: u8 = MC_NVRAM_START + 9;
#[allow(dead_code)]
pub(crate) const NVRAM_EXTHI: u8 = MC_NVRAM_START + 10;

// extended memory size (between 16 MB and 4GB)
// this register value comes from jos, but different from wiki?
#[allow(dead_code)]
pub(crate) const NVRAM_EXT16LO: u8 = MC_NVRAM_START + 38;
#[allow(dead_code)]
pub(crate) const NVRAM_EXT16HI: u8 = MC_NVRAM_START + 39;

/// Read the NVRAM register value from the real-time clock.
pub(crate) fn mc146818_read(reg: u8) -> u8 {
    x86::outb(IO_RTC, reg);
    x86::inb(IO_RTC + 1)
}

// /// Write the NVRAM register value from the real-time clock.
// pub(crate) fn mc146818_write(reg: u8, datum: u8) {
//     x86::outb(IO_RTC, reg);
//     x86::outb(IO_RTC + 1, datum);
// }
