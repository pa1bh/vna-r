//! Replace a file in one step, so that a write that fails half-way (a full
//! disk, the power going) leaves the file that was there instead of a
//! truncated one. A calibration file collects its standards one run at a
//! time: losing it to the write of the last one loses them all.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Write `contents` to a temporary file next to `path` and rename it over
/// `path`. Through a symbolic link the file it points at is replaced, not
/// the link.
pub fn write(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
    let target = if path.is_symlink() {
        fs::canonicalize(path)?
    } else {
        path.to_path_buf()
    };
    let tmp = temporary(&target);
    let written = (|| {
        {
            let mut f = File::create(&tmp)?;
            f.write_all(contents.as_ref())?;
            // On disk before it takes the place of the old file.
            f.sync_all()?;
            // And closed: Windows does not rename an open file.
        }
        fs::rename(&tmp, &target)
    })();
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    written
}

/// `.name.PID.tmp` in the same folder: a rename only replaces in one step
/// within a file system.
fn temporary(target: &Path) -> PathBuf {
    let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    target.with_file_name(format!(".{name}.{}.tmp", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vna-r-atomic-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_file_is_created_and_replaced_and_nothing_else_is_left() {
        let dir = scratch("replace");
        let path = dir.join("hf.cal.json");
        write(&path, "one").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "one");
        write(&path, b"two").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "two");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_write_that_fails_leaves_the_old_file() {
        let dir = scratch("fails");
        // a folder where the file should go: the rename fails
        let path = dir.join("taken");
        fs::create_dir(&path).unwrap();
        fs::write(path.join("inside"), "kept").unwrap();
        assert!(write(&path, "new").is_err());
        assert_eq!(fs::read_to_string(path.join("inside")).unwrap(), "kept");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1, "the temporary file is removed");
        // and a folder that is not there
        assert!(write(&dir.join("missing").join("x.json"), "new").is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_link_keeps_pointing_at_the_file() {
        let dir = scratch("link");
        let real = dir.join("real.json");
        let link = dir.join("link.json");
        fs::write(&real, "old").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        write(&link, "new").unwrap();
        assert!(link.is_symlink());
        assert_eq!(fs::read_to_string(&real).unwrap(), "new");
        fs::remove_dir_all(&dir).unwrap();
    }
}
