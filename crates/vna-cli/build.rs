//! Records the git revision at build time as `VNA_R_GIT_REV` (shown in About
//! and `--version`). A source tarball or a Nix build has no `.git`; those set
//! the variable themselves or get "unknown".

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=VNA_R_GIT_REV");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    let rev = std::env::var("VNA_R_GIT_REV").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| {
        let out = Command::new("git").args(["describe", "--always", "--dirty=-dirty"]).output();
        match out {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
            _ => "unknown".to_string(),
        }
    });
    println!("cargo:rustc-env=VNA_R_GIT_REV={rev}");
}
