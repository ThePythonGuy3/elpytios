use crate::io::{Read, Result, Write};

pub enum CopyState {
    #[cfg_attr(not(any(target_os = "linux", target_os = "android")), expect(dead_code))]
    Ended(u64),
    Fallback(u64),
}

pub fn kernel_copy<R: ?Sized, W: ?Sized>(_reader: &mut R, _writer: &mut W) -> Result<CopyState>
where
    R: Read,
    W: Write,
{
    Ok(CopyState::Fallback(0))
}
