#![deny(unsafe_op_in_unsafe_fn)]

use crate::{
    fs, io,
    path::{Path, PathBuf},
};

pub mod common;

mod imp {
    pub use crate::sys::fs::common::Dir;
    use crate::{
        ffi::OsString,
        fmt,
        fs::TryLockError,
        hash::{Hash, Hasher},
        io::{self, BorrowedCursor, IoSlice, IoSliceMut, SeekFrom},
        path::{Path, PathBuf},
        sys::time::SystemTime,
    };

    pub struct File(!);

    pub struct FileAttr(!);

    pub struct ReadDir(!);

    pub struct DirEntry(!);

    #[derive(Clone, Debug)]
    pub struct OpenOptions {}

    #[derive(Copy, Clone, Debug, Default)]
    pub struct FileTimes {}

    pub struct FilePermissions(!);

    pub struct FileType(!);

    #[derive(Debug)]
    pub struct DirBuilder {}

    impl FileAttr {
        pub fn size(&self) -> u64 {
            self.0
        }

        pub fn perm(&self) -> FilePermissions {
            self.0
        }

        pub fn file_type(&self) -> FileType {
            self.0
        }

        pub fn modified(&self) -> io::Result<SystemTime> {
            self.0
        }

        pub fn accessed(&self) -> io::Result<SystemTime> {
            self.0
        }

        pub fn created(&self) -> io::Result<SystemTime> {
            self.0
        }
    }

    impl Clone for FileAttr {
        fn clone(&self) -> FileAttr {
            self.0
        }
    }

    impl FilePermissions {
        pub fn readonly(&self) -> bool {
            self.0
        }

        pub fn set_readonly(&mut self, _readonly: bool) {
            self.0
        }
    }

    impl Clone for FilePermissions {
        fn clone(&self) -> FilePermissions {
            self.0
        }
    }

    impl PartialEq for FilePermissions {
        fn eq(&self, _other: &FilePermissions) -> bool {
            self.0
        }
    }

    impl Eq for FilePermissions {}

    impl fmt::Debug for FilePermissions {
        fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0
        }
    }

    impl FileTimes {
        pub fn set_accessed(&mut self, _t: SystemTime) {}
        pub fn set_modified(&mut self, _t: SystemTime) {}
    }

    impl FileType {
        pub fn is_dir(&self) -> bool {
            self.0
        }

        pub fn is_file(&self) -> bool {
            self.0
        }

        pub fn is_symlink(&self) -> bool {
            self.0
        }
    }

    impl Clone for FileType {
        fn clone(&self) -> FileType {
            self.0
        }
    }

    impl Copy for FileType {}

    impl PartialEq for FileType {
        fn eq(&self, _other: &FileType) -> bool {
            self.0
        }
    }

    impl Eq for FileType {}

    impl Hash for FileType {
        fn hash<H: Hasher>(&self, _h: &mut H) {
            self.0
        }
    }

    impl fmt::Debug for FileType {
        fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0
        }
    }

    impl fmt::Debug for ReadDir {
        fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0
        }
    }

    impl Iterator for ReadDir {
        type Item = io::Result<DirEntry>;

        fn next(&mut self) -> Option<io::Result<DirEntry>> {
            self.0
        }
    }

    impl DirEntry {
        pub fn path(&self) -> PathBuf {
            self.0
        }

        pub fn file_name(&self) -> OsString {
            self.0
        }

        pub fn metadata(&self) -> io::Result<FileAttr> {
            self.0
        }

        pub fn file_type(&self) -> io::Result<FileType> {
            self.0
        }
    }

    impl OpenOptions {
        pub fn new() -> OpenOptions {
            OpenOptions {}
        }

        pub fn read(&mut self, _read: bool) {}
        pub fn write(&mut self, _write: bool) {}
        pub fn append(&mut self, _append: bool) {}
        pub fn truncate(&mut self, _truncate: bool) {}
        pub fn create(&mut self, _create: bool) {}
        pub fn create_new(&mut self, _create_new: bool) {}
    }

    impl File {
        pub fn open(_path: &Path, _opts: &OpenOptions) -> io::Result<File> {
            unimplemented!()
        }

        pub fn file_attr(&self) -> io::Result<FileAttr> {
            self.0
        }

        pub fn fsync(&self) -> io::Result<()> {
            self.0
        }

        pub fn datasync(&self) -> io::Result<()> {
            self.0
        }

        pub fn lock(&self) -> io::Result<()> {
            self.0
        }

        pub fn lock_shared(&self) -> io::Result<()> {
            self.0
        }

        pub fn try_lock(&self) -> Result<(), TryLockError> {
            self.0
        }

        pub fn try_lock_shared(&self) -> Result<(), TryLockError> {
            self.0
        }

        pub fn unlock(&self) -> io::Result<()> {
            self.0
        }

        pub fn truncate(&self, _size: u64) -> io::Result<()> {
            self.0
        }

        pub fn read(&self, _buf: &mut [u8]) -> io::Result<usize> {
            self.0
        }

        pub fn read_vectored(&self, _bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
            self.0
        }

        pub fn is_read_vectored(&self) -> bool {
            self.0
        }

        pub fn read_buf(&self, _cursor: BorrowedCursor<'_, u8>) -> io::Result<()> {
            self.0
        }

        pub fn write(&self, _buf: &[u8]) -> io::Result<usize> {
            self.0
        }

        pub fn write_vectored(&self, _bufs: &[IoSlice<'_>]) -> io::Result<usize> {
            self.0
        }

        pub fn is_write_vectored(&self) -> bool {
            self.0
        }

        pub fn flush(&self) -> io::Result<()> {
            self.0
        }

        pub fn seek(&self, _pos: SeekFrom) -> io::Result<u64> {
            self.0
        }

        pub fn size(&self) -> Option<io::Result<u64>> {
            self.0
        }

        pub fn tell(&self) -> io::Result<u64> {
            self.0
        }

        pub fn duplicate(&self) -> io::Result<File> {
            self.0
        }

        pub fn set_permissions(&self, _perm: FilePermissions) -> io::Result<()> {
            self.0
        }

        pub fn set_times(&self, _times: FileTimes) -> io::Result<()> {
            self.0
        }
    }

    impl DirBuilder {
        pub fn new() -> DirBuilder {
            DirBuilder {}
        }

        pub fn mkdir(&self, _p: &Path) -> io::Result<()> {
            unimplemented!()
        }
    }

    impl fmt::Debug for File {
        fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0
        }
    }

    pub fn readdir(_p: &Path) -> io::Result<ReadDir> {
        unimplemented!()
    }

    pub fn unlink(_p: &Path) -> io::Result<()> {
        unimplemented!()
    }

    pub fn rename(_old: &Path, _new: &Path) -> io::Result<()> {
        unimplemented!()
    }

    pub fn set_perm(_p: &Path, perm: FilePermissions) -> io::Result<()> {
        match perm.0 {}
    }

    pub fn set_times(_p: &Path, _times: FileTimes) -> io::Result<()> {
        unimplemented!()
    }

    pub fn set_times_nofollow(_p: &Path, _times: FileTimes) -> io::Result<()> {
        unimplemented!()
    }

    pub fn rmdir(_p: &Path) -> io::Result<()> {
        unimplemented!()
    }

    pub fn remove_dir_all(_path: &Path) -> io::Result<()> {
        unimplemented!()
    }

    pub fn exists(_path: &Path) -> io::Result<bool> {
        unimplemented!()
    }

    pub fn readlink(_p: &Path) -> io::Result<PathBuf> {
        unimplemented!()
    }

    pub fn symlink(_original: &Path, _link: &Path) -> io::Result<()> {
        unimplemented!()
    }

    pub fn link(_src: &Path, _dst: &Path) -> io::Result<()> {
        unimplemented!()
    }

    pub fn stat(_p: &Path) -> io::Result<FileAttr> {
        unimplemented!()
    }

    pub fn lstat(_p: &Path) -> io::Result<FileAttr> {
        unimplemented!()
    }

    pub fn canonicalize(_p: &Path) -> io::Result<PathBuf> {
        unimplemented!()
    }

    pub fn copy(_from: &Path, _to: &Path) -> io::Result<u64> {
        unimplemented!()
    }
}

// FIXME: Replace this with platform-specific path conversion functions.
#[inline]
pub fn with_native_path<T>(path: &Path, f: &dyn Fn(&Path) -> io::Result<T>) -> io::Result<T> {
    f(path)
}

pub use imp::{Dir, DirBuilder, DirEntry, File, FileAttr, FilePermissions, FileTimes, FileType, OpenOptions, ReadDir};

pub fn read_dir(path: &Path) -> io::Result<ReadDir> {
    // FIXME: use with_native_path on all platforms
    imp::readdir(path)
}

pub fn remove_file(path: &Path) -> io::Result<()> {
    with_native_path(path, &imp::unlink)
}

pub fn rename(old: &Path, new: &Path) -> io::Result<()> {
    with_native_path(old, &|old| with_native_path(new, &|new| imp::rename(old, new)))
}

pub fn remove_dir(path: &Path) -> io::Result<()> {
    with_native_path(path, &imp::rmdir)
}

pub fn remove_dir_all(path: &Path) -> io::Result<()> {
    imp::remove_dir_all(path)
}

pub fn read_link(path: &Path) -> io::Result<PathBuf> {
    with_native_path(path, &imp::readlink)
}

pub fn symlink(original: &Path, link: &Path) -> io::Result<()> {
    with_native_path(original, &|original| with_native_path(link, &|link| imp::symlink(original, link)))
}

pub fn hard_link(original: &Path, link: &Path) -> io::Result<()> {
    with_native_path(original, &|original| with_native_path(link, &|link| imp::link(original, link)))
}

pub fn metadata(path: &Path) -> io::Result<FileAttr> {
    with_native_path(path, &imp::stat)
}

pub fn symlink_metadata(path: &Path) -> io::Result<FileAttr> {
    with_native_path(path, &imp::lstat)
}

pub fn set_permissions(path: &Path, perm: FilePermissions) -> io::Result<()> {
    with_native_path(path, &|path| imp::set_perm(path, perm.clone()))
}

pub fn set_permissions_nofollow(_path: &Path, _perm: fs::Permissions) -> io::Result<()> {
    unimplemented!("`set_permissions_nofollow` is currently only implemented on Unix platforms")
}

pub fn canonicalize(path: &Path) -> io::Result<PathBuf> {
    with_native_path(path, &imp::canonicalize)
}

pub fn copy(from: &Path, to: &Path) -> io::Result<u64> {
    imp::copy(from, to)
}

pub fn exists(path: &Path) -> io::Result<bool> {
    imp::exists(path)
}

pub fn set_times(path: &Path, times: FileTimes) -> io::Result<()> {
    with_native_path(path, &|path| imp::set_times(path, times.clone()))
}

pub fn set_times_nofollow(path: &Path, times: FileTimes) -> io::Result<()> {
    with_native_path(path, &|path| imp::set_times_nofollow(path, times.clone()))
}
