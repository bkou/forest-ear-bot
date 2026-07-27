//! Records the git revision at build time so the running bot can report exactly
//! what it was built from.

use std::process::Command;

fn main() {
    // Without these, cargo caches this script's output and the hash goes stale.
    // HEAD covers checkouts; the ref it points at covers new commits; src covers
    // edits, which matter for the `-dirty` marker.
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=src");
    if let Ok(head) = std::fs::read_to_string(".git/HEAD") {
        if let Some(reference) = head.strip_prefix("ref: ") {
            println!("cargo:rerun-if-changed=.git/{}", reference.trim());
        }
    }

    println!("cargo:rustc-env=GIT_HASH={}", describe());
}

/// The short hash, marked `-dirty` when the tree had uncommitted changes.
fn describe() -> String {
    let Some(hash) = git(&["rev-parse", "--short", "HEAD"]) else {
        // No git on PATH, or building from a source archive rather than a clone.
        return String::from("unknown");
    };

    match git(&["status", "--porcelain"]) {
        Some(changes) if !changes.is_empty() => format!("{}-dirty", hash),
        _ => hash,
    }
}

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
