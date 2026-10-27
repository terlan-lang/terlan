//! Runtime-owned resources remain alive until all application owners stop.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};

use super::*;

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(1);

pub(crate) struct ApplicationFiles {
    root: PathBuf,
}

impl ApplicationFiles {
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
}

impl Drop for ApplicationFiles {
    fn drop(&mut self) {
        // Windows does not remove read-only files with remove_dir_all.
        #[cfg(windows)]
        fn writable(directory: &Path) {
            if let Ok(entries) = fs::read_dir(directory) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let Ok(kind) = entry.file_type() else {
                        continue;
                    };
                    if kind.is_dir() {
                        writable(&path);
                    } else if kind.is_file() {
                        if let Ok(metadata) = path.metadata() {
                            let mut permissions = metadata.permissions();
                            permissions.set_readonly(false);
                            let _ = fs::set_permissions(&path, permissions);
                        }
                    }
                }
            }
        }
        #[cfg(windows)]
        writable(&self.root);
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl EmbeddedApplication {
    /// Produces only validated regular files inside an exclusively owned root.
    pub(crate) fn materialize(&mut self) -> Result<ApplicationFiles, String> {
        let files = ApplicationFiles {
            root: private_directory()?,
        };
        for resource in &self.manifest.files {
            let path = files.root.join(&resource.path);
            fs::create_dir_all(path.parent().unwrap()).map_err(io_error)?;
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(io_error)?;
            self.file
                .seek(SeekFrom::Start(self.payload_start + resource.offset))
                .map_err(io_error)?;
            let mut input = Read::by_ref(&mut self.file).take(resource.length);
            let mut digest = Sha256::new();
            let mut copied = 0_u64;
            let mut buffer = [0; 65_536];
            loop {
                let read = input.read(&mut buffer).map_err(io_error)?;
                if read == 0 {
                    break;
                }
                output.write_all(&buffer[..read]).map_err(io_error)?;
                digest.update(&buffer[..read]);
                copied += read as u64;
            }
            if copied != resource.length || <[u8; 32]>::from(digest.finalize()) != resource.sha256 {
                return Err(error("digest", "embedded resource bytes changed"));
            }
            output.sync_all().map_err(io_error)?;
            drop(output);
            let mut permissions = fs::metadata(&path).map_err(io_error)?.permissions();
            permissions.set_readonly(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if resource.executable {
                    permissions.set_mode(0o500);
                }
            }
            fs::set_permissions(&path, permissions).map_err(io_error)?;
        }
        Ok(files)
    }
}

fn private_directory() -> Result<PathBuf, String> {
    for _ in 0..128 {
        let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "terlan-application-{}-{sequence}",
            std::process::id()
        ));
        #[cfg(unix)]
        let mut builder = fs::DirBuilder::new();
        #[cfg(not(unix))]
        let builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        match builder.create(&path) {
            Ok(()) => return Ok(path),
            Err(cause) if cause.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(cause) => return Err(io_error(cause)),
        }
    }
    Err(error(
        "directory",
        "cannot allocate a private application directory",
    ))
}
