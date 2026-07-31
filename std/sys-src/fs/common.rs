use crate::{
    fmt, io,
    path::{Path, PathBuf},
    sys::{
        IntoInner,
        fs::{File, FileAttr, OpenOptions},
    },
};

pub struct Dir {
    path: PathBuf,
}

impl Dir {
    pub fn open(path: &Path, _opts: &OpenOptions) -> io::Result<Self> {
        path.canonicalize().map(|path| Self { path })
    }

    pub fn open_file(&self, path: &Path, opts: &OpenOptions) -> io::Result<File> {
        File::open(&self.path.join(path), &opts)
    }

    pub fn metadata(&self) -> io::Result<FileAttr> {
        self.path.metadata().map(|m| m.into_inner())
    }

    pub fn remove_file(&self, path: &Path) -> io::Result<()> {
        crate::fs::remove_file(self.path.join(path))
    }

    pub fn rename(&self, from: &Path, to_dir: &Self, to: &Path) -> io::Result<()> {
        crate::fs::rename(self.path.join(from), to_dir.path.join(to))
    }
}

impl fmt::Debug for Dir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Dir").field("path", &self.path).finish()
    }
}
