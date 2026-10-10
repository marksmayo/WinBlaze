use std::{env, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=cli.manifest");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("crate directory"))
            .join("cli.manifest");
        // Keep the console subsystem's shell wait/exit contract. Supported
        // Windows versions can avoid a new console for unattended launches.
        println!("cargo:rustc-link-arg-bin=winblaze-cli=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-bin=winblaze-cli=/MANIFESTINPUT:{}",
            manifest.display()
        );
    }
}
