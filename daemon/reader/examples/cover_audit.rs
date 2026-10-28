//! Report audio files whose album art cannot be resolved locally.
//!
//! A track counts as covered when its tags embed a picture, it has a sidecar
//! image next to it (`song.jpg`), or its folder has a `cover`/`folder` image.
//! Remote fetching is deliberately not attempted: this audits what a library
//! scan can show before any network is involved.
//!
//! Usage: `cargo run -p kopuz-reader --example cover_audit -- [DIR]`

#![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "the audit is a command-line report"
)]

use std::path::{Path, PathBuf};

use lofty::file::TaggedFileExt;

fn main() {
    let root = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    let mut audio_files = Vec::new();
    collect_audio_files(&root, &mut audio_files);
    audio_files.sort_unstable();

    let mut missing = Vec::new();
    let mut unreadable = Vec::new();

    for path in &audio_files {
        if reader::read_metadata(path).is_none() {
            unreadable.push(path);
            continue;
        }
        if !has_local_picture(path) {
            missing.push(path);
        }
    }

    for path in &missing {
        println!("{}", path.display());
    }

    println!();
    println!(
        "{} audio files, {} without a picture, {} whose metadata could not be read",
        audio_files.len(),
        missing.len(),
        unreadable.len()
    );

    for path in &unreadable {
        eprintln!("unreadable: {}", path.display());
    }
}

fn collect_audio_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_audio_files(&path, out);
        } else if reader::scanner::is_audio_file(&path) {
            out.push(path);
        }
    }
}

/// Mirrors `cover_indexer::resolve_local_cover` without touching the cover cache.
fn has_local_picture(path: &Path) -> bool {
    if embedded_picture(path) {
        return true;
    }

    let Some(parent) = path.parent() else {
        return false;
    };
    if let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) {
        for extension in ["jpg", "jpeg", "png", "webp"] {
            if parent.join(stem).with_extension(extension).is_file() {
                return true;
            }
        }
    }
    reader::utils::find_folder_cover(parent).is_some()
}

fn embedded_picture(path: &Path) -> bool {
    let Ok(tagged) = lofty::probe::Probe::open(path).and_then(lofty::probe::Probe::read) else {
        return false;
    };
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());
    reader::metadata::extract_embedded_cover(&tagged, tag).is_some()
}
