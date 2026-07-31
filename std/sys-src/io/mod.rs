mod error;

mod is_terminal {
    pub fn is_terminal<T>(_: &T) -> bool {
        false
    }
}

mod kernel_copy;

pub use error::{decode_error_kind, errno, error_string, is_interrupted};
pub use is_terminal::is_terminal;
pub use kernel_copy::{CopyState, kernel_copy};
