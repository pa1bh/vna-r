//! The git revision part of this build script is shared with the other binaries: see `../build_git_rev.rs`.

include!("../build_git_rev.rs");

fn main() {
    record_git_rev();
}
