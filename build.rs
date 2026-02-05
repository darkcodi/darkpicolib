//! This build script copies the `memory.x` file from the crate root into
//! a directory where the linker can always find it at build time.
//! For many projects this is optional, as the linker always searches the
//! project root directory -- wherever `Cargo.toml` is. However, if you
//! are using a workspace or have a more complicated build setup, this
//! build script becomes required. Additionally, by requesting that
//! Cargo re-run the build script whenever `memory.x` is changed,
//! updating `memory.x` ensures a rebuild of the application with the
//! new memory settings.

use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;

fn main() {
    // Load .env file for compile-time environment variables
    match fs::read_to_string(".env") {
        Ok(content) => {
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((key, value)) = line.split_once('=') {
                    let key = key.trim();
                    let value = value.trim().trim_matches('"');
                    // Only set if not already set (allows overriding from shell)
                    if env::var(key).is_err() {
                        println!("cargo:rustc-env={}={}", key, value);
                    }
                }
            }
        }
        Err(_) => {
            // .env file doesn't exist - that's ok, user can set vars externally
        }
    }

    // Put `memory.x` in our output directory and ensure it's
    // on the linker search path.
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    File::create(out.join("memory.x"))
        .unwrap()
        .write_all(include_bytes!("memory.x"))
        .unwrap();
    println!("cargo:rustc-link-search={}", out.display());

    // Re-run when these files change
    println!("cargo:rerun-if-changed=memory.x");
    println!("cargo:rerun-if-changed=.env");
}
