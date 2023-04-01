//! System parameters and memory layout.
// Qemu riscv virt machine memory locations you want to know:
// https://github.com/qemu/qemu/blob/master/hw/riscv/virt.c
//
//static static MemMapEntry virt_memmap[] = {
//    [VIRT_DEBUG] =        {        0x0,         0x100 },
//    [VIRT_MROM] =         {     0x1000,        0xf000 },
//    [VIRT_TEST] =         {   0x100000,        0x1000 },
//    [VIRT_RTC] =          {   0x101000,        0x1000 },
//    [VIRT_CLINT] =        {  0x2000000,       0x10000 },
//    [VIRT_ACLINT_SSWI] =  {  0x2F00000,        0x4000 },
//    [VIRT_PCIE_PIO] =     {  0x3000000,       0x10000 },
//    [VIRT_PLATFORM_BUS] = {  0x4000000,     0x2000000 },
//    [VIRT_PLIC] =         {  0xc000000, VIRT_PLIC_SIZE(VIRT_CPUS_MAX * 2) },
//    [VIRT_APLIC_M] =      {  0xc000000, APLIC_SIZE(VIRT_CPUS_MAX) },
//    [VIRT_APLIC_S] =      {  0xd000000, APLIC_SIZE(VIRT_CPUS_MAX) },
//    [VIRT_UART0] =        { 0x10000000,         0x100 },
//    [VIRT_VIRTIO] =       { 0x10001000,        0x1000 },
//    [VIRT_FW_CFG] =       { 0x10100000,          0x18 },
//    [VIRT_FLASH] =        { 0x20000000,     0x4000000 },
//    [VIRT_IMSIC_M] =      { 0x24000000, VIRT_IMSIC_MAX_SIZE },
//    [VIRT_IMSIC_S] =      { 0x28000000, VIRT_IMSIC_MAX_SIZE },
//    [VIRT_PCIE_ECAM] =    { 0x30000000,    0x10000000 },
//    [VIRT_PCIE_MMIO] =    { 0x40000000,    0x40000000 },
//    [VIRT_DRAM] =         { 0x80000000,           0x0 },
//}

use core::ptr::addr_of_mut;

// NOTE:
// We can't just use link_name for linker symbols, cause they don't
// bind correctly for some reason.
// Instead, use core::ptr::addr_of!() to get address and then cast to usize.
//
// TODO consider reworking this to have a consistent naming scheme and
// maybe a macro for the getter functions.
extern "C" {
    static mut _text_end: usize;
    static mut _bss_start: usize;
    static mut _bss_end: usize;
    static mut _memory_end: usize;
    static mut _roedata: usize;
    static mut _edata: usize;
    static mut _stacks_start: usize;
    static mut _stacks_end: usize;
    static mut _intstacks_start: usize;
    static mut _intstacks_end: usize;
}

/// CLINT base address.
pub const CLINT_BASE: usize = 0x2000000;

/// UART base adderss.
pub const UART_BASE: usize = 0x10000000;

/// PLIC base adderss.
pub const PLIC_BASE: usize = 0x0c000000;

/// Start of kernel memory (first .text section goes here).
pub const DRAM_BASE: *mut usize = 0x80000000 as *mut usize;

pub fn text_end() -> *mut usize {
    unsafe { addr_of_mut!(_text_end) }
}

pub fn bss_end() -> *mut usize {
    unsafe { addr_of_mut!(_bss_end) }
}

pub fn bss_start() -> *mut usize {
    unsafe { addr_of_mut!(_bss_start) }
}

pub fn rodata_end() -> *mut usize {
    unsafe { addr_of_mut!(_roedata) }
}

pub fn data_end() -> *mut usize {
    unsafe { addr_of_mut!(_edata) }
}

pub fn stacks_start() -> *mut usize {
    unsafe { addr_of_mut!(_stacks_start) }
}

pub fn stacks_end() -> *mut usize {
    unsafe { addr_of_mut!(_stacks_end) }
}

pub fn intstacks_start() -> *mut usize {
    unsafe { addr_of_mut!(_intstacks_start) }
}

pub fn intstacks_end() -> *mut usize {
    unsafe { addr_of_mut!(_intstacks_end) }
}

pub fn dram_end() -> *mut usize {
    unsafe { addr_of_mut!(_memory_end) }
}

pub static PAGE_SIZE: usize = 4096;

// Run parameters
pub const NHART: usize = 2;

// Unnecessary.
pub static BANNER: &str = r#"
Mellow Swirled,
                       __
   ________  ___  ____/ /___  _____
  / ___/ _ \/ _ \/ __  / __ \/ ___/
 / /  /  __/  __/ /_/ / /_/ (__  )
/_/   \___/\___/\__,_/\____/____/

"#;
