use std::fs;
use std::io::{self, Error, ErrorKind};
use std::path::{Path, PathBuf};

/// Default generated file name in `OUT_DIR`.
pub const DEFAULT_EMBED_FILE: &str = "agent_doc_pages.rs";

/// Scan `dir` recursively for `.md` files and generate an array of `(path, include_str!(...))`
/// into `$OUT_DIR/agent_doc_pages.rs`.
///
/// Emits `cargo:rerun-if-changed` for `dir` and all scanned markdown files.
pub fn embed(dir: impl AsRef<Path>) -> io::Result<()> {
    let out_dir = std::env::var("OUT_DIR").map_err(|e| {
        Error::new(
            ErrorKind::NotFound,
            format!("OUT_DIR environment variable not found: {e}"),
        )
    })?;
    embed_to(dir, Path::new(&out_dir).join(DEFAULT_EMBED_FILE))
}

/// Scan `dir` recursively for `.md` files and write the generated code to `target_file`.
pub fn embed_to(dir: impl AsRef<Path>, target_file: impl AsRef<Path>) -> io::Result<()> {
    let dir = dir.as_ref();
    let mut files = Vec::new();
    collect_md_files(dir, &mut files)?;
    files.sort();

    // Output cargo rerun directives for build script
    println!("cargo:rerun-if-changed={}", dir.display());
    for file in &files {
        println!("cargo:rerun-if-changed={}", file.display());
    }

    let mut out = String::new();
    out.push_str("[\n");
    for file in &files {
        let abs_path = if file.is_absolute() {
            file.canonicalize().unwrap_or_else(|_| file.clone())
        } else {
            std::env::current_dir()
                .map(|cd| cd.join(file))
                .and_then(|p| p.canonicalize())
                .unwrap_or_else(|_| file.clone())
        };

        let display_path = file.to_string_lossy();
        let abs_path_str = abs_path.to_string_lossy();

        out.push_str(&format!(
            "    ({:?}, include_str!(r#\"{}\"#)),\n",
            display_path, abs_path_str
        ));
    }
    out.push_str("]\n");

    let target_path = target_file.as_ref();
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(target_path, out)?;
    Ok(())
}

fn collect_md_files(dir: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_md_files(&path, files)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("md") {
            files.push(path);
        }
    }
    Ok(())
}
