use crate::{
    ffi::OsString,
    io::{Error, Result},
};

pub fn hostname() -> Result<OsString> {
    Err(Error::UNSUPPORTED_PLATFORM)
}
