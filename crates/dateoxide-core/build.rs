use std::env;
use std::fs;
use std::path::PathBuf;

include!("src/locale/schema.rs");

fn main() {
    println!("cargo:rerun-if-changed=data/locales.json");
    println!("cargo:rerun-if-changed=src/locale/schema.rs");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let json_path = manifest_dir.join("data").join("locales.json");
    let json_str = fs::read_to_string(&json_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", json_path.display(), e));

    let locales: Vec<RawLocale> = serde_json::from_str(&json_str)
        .unwrap_or_else(|e| panic!("failed to parse locales.json: {}", e));

    let bytes = postcard::to_allocvec(&locales)
        .unwrap_or_else(|e| panic!("failed to serialize locales.bin: {}", e));

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let bin_path = out_dir.join("locales.bin");
    fs::write(&bin_path, &bytes)
        .unwrap_or_else(|e| panic!("failed to write {}: {}", bin_path.display(), e));
}
