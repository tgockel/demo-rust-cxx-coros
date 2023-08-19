extern crate cbindgen;

use std::{env, fs, path::PathBuf};

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_file = PathBuf::from(env::var("OUT_DIR").unwrap()).join("cachers.h");

    println!("cargo:rerun-if-changed=cbindgen.toml");
    println!("cargo:rerun-if-changed=src");
    cbindgen::Builder::new()
        .with_config(cbindgen::Config::from_file("cbindgen.toml").expect("failed to load config"))
        .with_crate(crate_dir)
        .generate()
        .expect("unable to generate bindings")
        .write_to_file(&out_file);

    // HACK: Write the file to the output directory, since Cargo will properly rebuild that, then always copy to the
    // location the C++ project can find it.
    fs::copy(out_file, "../src/cachers.h").expect("failed to copy generated output");
}
