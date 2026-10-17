use std::{env, path::PathBuf, process::Command};

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest.join("../..");
    let prefix = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("vt");
    println!("cargo:rerun-if-changed=scripts/build-vt.sh");
    println!("cargo:rerun-if-changed=../../vendor/ghostty/src");
    println!("cargo:rerun-if-changed=../../vendor/ghostty/include");
    println!("cargo:rerun-if-changed=../../vendor/ghostty/build.zig");
    println!("cargo:rerun-if-env-changed=GAP_ZIG");
    let zig = env::var_os("GAP_ZIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join(".tools/zig/zig"));
    assert!(
        zig.is_file(),
        "Run scripts/bootstrap-native.sh to install project-local Zig 0.16.0"
    );
    let status = Command::new("bash")
        .arg(manifest.join("scripts/build-vt.sh"))
        .arg(&prefix)
        .env("GAP_ZIG", zig)
        .status()
        .expect("build libghostty-vt");
    assert!(status.success(), "libghostty-vt build failed");
    println!(
        "cargo:rustc-link-search=native={}",
        prefix.join("lib").display()
    );
    println!("cargo:rustc-link-lib=static=ghostty-vt");
    println!("cargo:rustc-link-lib=c++");
}
