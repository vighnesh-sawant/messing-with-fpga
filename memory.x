MEMORY {
    /* * RP2040 Boot Loader (Second Stage) 
     * This is the small shim that sets up the flash XIP before your main code runs.
     */
    BOOT2 : ORIGIN = 0x10000000, LENGTH = 0x100

    /* * Main Executable Flash 
     * Total Flash is 4MB (0x400000).
     * We reserved the upper 2MB (starting at offset 0x200000) for FPGA Bitstream storage.
     * Therefore, we limit the Code Flash to the first 2MB minus the BOOT2 size.
     * * Origin = 0x10000100 (Start of flash + 256 bytes for boot2)
     * Length = 2MB (0x200000) - 256 bytes (0x100) = 0x1FFFFF
     */
    FLASH : ORIGIN = 0x10000100, LENGTH = 0x1FFFFF

    /* * RAM 
     * The RP2040 has 264KB of SRAM.
     */
    RAM   : ORIGIN = 0x20000000, LENGTH = 264K
}

/* * This symbol is used by the `cortex-m-rt` crate to initialize the stack. 
 * Placing it at the end of RAM maximizes stack space.
 */
_stack_start = ORIGIN(RAM) + LENGTH(RAM);
