#![no_std]
#![expect(non_camel_case_types, reason = "Matching `<elf.h>` header")]

pub mod sys;

mod reader;
pub use reader::*;

pub struct Elf64<'a> {
    //
}

impl<'a> Elf64<'a> {
    //pub fn from_bytes(bytes: &'a [u8]) -> Self {}
}
