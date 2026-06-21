#![expect(non_camel_case_types, reason = "Matching `<elf.h>` header")]

mod elf_header;
pub use elf_header::*;

mod program_header;
pub use program_header::*;
