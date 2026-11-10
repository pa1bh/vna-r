// Shared build script of vna-cli and vna-gui (each `build.rs` includes this file).
//
// Records the git revision at build time as `VNA_R_GIT_REV` (shown in About
// and `--version`). A source tarball or a Nix build has no `.git`; those set
// the variable themselves or get "unknown".

use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=VNA_R_GIT_REV");
    // Re-run when the revision can have changed. `HEAD` itself only changes on
    // a branch switch; a new commit moves the branch ref it points at (or
    // `packed-refs` after a gc), and the index changes with every `git add`,
    // which is what turns the revision "-dirty". A path that does not exist
    // would make cargo re-run the script on every build, so only name those
    // that are there.
    let git = Path::new("../../.git");
    let mut watch = vec![git.join("HEAD"), git.join("packed-refs"), git.join("index")];
    if let Ok(head) = std::fs::read_to_string(git.join("HEAD")) {
        if let Some(reference) = head.trim().strip_prefix("ref: ") {
            watch.push(git.join(reference));
        }
    }
    for path in watch.iter().filter(|p| p.exists()) {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    let rev = std::env::var("VNA_R_GIT_REV").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| {
        let out = Command::new("git").args(["describe", "--always", "--dirty=-dirty"]).output();
        match out {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
            _ => "unknown".to_string(),
        }
    });
    println!("cargo:rustc-env=VNA_R_GIT_REV={rev}");
}
