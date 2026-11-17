//! Two jobs: the git revision for `--version` (shared with the other
//! binaries, see `../build_git_rev.rs`), and the built-in copy of the web
//! client.
//!
//! The web client is the output of `scripts/build-web.sh`: `index.html`,
//! `vna-gui.js` and `vna-gui_bg.wasm` in `<workspace>/target/web`, or in
//! `$VNA_R_WEB_DIR` when that is set (for packaging). When the three are
//! there they are embedded, the two big ones gzip-compressed; otherwise a
//! placeholder page that says how to get them. A checkout without the wasm
//! build therefore still builds.

include!("../build_git_rev.rs");

use std::fs;
use std::io::Write as _;
use std::path::PathBuf; // and Path, from build_git_rev.rs

use flate2::write::GzEncoder;
use flate2::Compression;

const FILES: [&str; 3] = ["index.html", "vna-gui.js", "vna-gui_bg.wasm"];

fn main() {
    record_git_rev();
    embed_web();
}

fn embed_web() {
    println!("cargo:rerun-if-env-changed=VNA_R_WEB_DIR");
    println!("cargo:rerun-if-changed=src/placeholder.html");
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let explicit = std::env::var("VNA_R_WEB_DIR").ok().filter(|s| !s.is_empty());
    let dir = match &explicit {
        Some(d) => PathBuf::from(d),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/web"),
    };
    // Cargo re-runs a build script on every build when it watches a path that
    // does not exist, so make the default folder exist (it is build output
    // anyway) and watch it: build-web.sh replaces it, which cargo then sees.
    if explicit.is_none() {
        let _ = fs::create_dir_all(&dir);
    }
    if dir.is_dir() {
        println!("cargo:rerun-if-changed={}", dir.display());
    }
    let present: Vec<&str> = FILES.iter().copied().filter(|f| dir.join(f).is_file()).collect();
    let code = if present.len() == FILES.len() {
        let index = out.join("index.html");
        fs::copy(dir.join("index.html"), &index).expect("copy index.html");
        let js = gzip(&dir.join("vna-gui.js"), &out.join("vna-gui.js.gz"));
        let wasm = gzip(&dir.join("vna-gui_bg.wasm"), &out.join("vna-gui_bg.wasm.gz"));
        format!(
            "pub(crate) const BUILT_IN: bool = true;\n\
             pub(crate) const INDEX: &[u8] = include_bytes!({index:?});\n\
             pub(crate) const JS: Option<Packed> = Some({js});\n\
             pub(crate) const WASM: Option<Packed> = Some({wasm});\n",
            index = index.display().to_string(),
        )
    } else {
        if let Some(d) = &explicit {
            panic!("VNA_R_WEB_DIR={d} lacks some of {FILES:?}: scripts/build-web.sh writes them");
        }
        if !present.is_empty() {
            println!(
                "cargo:warning={} holds only {present:?} of {FILES:?}; the server gets the placeholder page",
                dir.display()
            );
        }
        let placeholder = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/placeholder.html");
        format!(
            "pub(crate) const BUILT_IN: bool = false;\n\
             pub(crate) const INDEX: &[u8] = include_bytes!({:?});\n\
             pub(crate) const JS: Option<Packed> = None;\n\
             pub(crate) const WASM: Option<Packed> = None;\n",
            placeholder.display().to_string()
        )
    };
    fs::write(out.join("web_assets.rs"), code).expect("write web_assets.rs");
}

/// Compress `from` into `to`; the Rust expression of the `Packed` that embeds it.
fn gzip(from: &Path, to: &Path) -> String {
    let raw = fs::read(from).unwrap_or_else(|e| panic!("{}: {e}", from.display()));
    let mut enc = GzEncoder::new(Vec::new(), Compression::best());
    enc.write_all(&raw).expect("gzip");
    fs::write(to, enc.finish().expect("gzip")).expect("write compressed file");
    format!(
        "Packed {{ gz: include_bytes!({:?}), len: {} }}",
        to.display().to_string(),
        raw.len()
    )
}
