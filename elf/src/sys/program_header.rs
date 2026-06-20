/*
Position 	Value
0-3 	Type of segment (see below)
4-7 	Flags (see below)
8-15 	The offset in the file that the data for this segment can be found (p_offset)
16-23 	Where you should start to put this segment in virtual memory (p_vaddr)
24-31 	Reserved for segment's physical address (p_paddr)
32-39 	Size of the segment in the file (p_filesz)
40-47 	Size of the segment in memory (p_memsz, at least as big as p_filesz)
48-55 	The required alignment for this section (usually a power of 2)

Segment types: 0 = null - ignore the entry; 1 = load - clear p_memsz bytes at p_vaddr to 0, then copy p_filesz bytes from p_offset to p_vaddr; 2 = dynamic - requires dynamic linking; 3 = interp - contains a file path to an executable to use as an interpreter for the following segment; 4 = note section. There are more values, but mostly contain architecture/environment specific information, which is probably not required for the majority of ELF files.

Flags: 1 = executable, 2 = writable, 4 = readable.
*/

#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct ProgramHeader {
    /// Type of segment (0 = null; 1 = load; 2 = dynamic; 3 = interp; 4 = note section) 0-3
    p_type: u32,
    /// Flags (1 = executable, 2 = writable, 4 = readable)                              4-7
    p_flags: u32,
    /// The offset in the file that the data for this segment can be found              8-15
    p_offset: u64,
    /// Where you should start to put this segment in virtual memory                    16-23
    p_vaddr: u64,
    /// Reserved for segment's physical address                                         24-31
    p_paddr: u64,
    /// Size of the segment in the file                                                 32-39
    p_filesz: u64,
    /// Size of the segment in memory (>= p_filesz; pad with zeroes)                    40-47
    p_memsz: u64,
    /// The required alignment for this section                                         48-55
    p_align: u64,
}
