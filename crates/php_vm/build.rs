use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let crates_root = manifest.parent().expect("php_vm is under crates/");
    let inputs = [
        manifest.join("src"),
        manifest.join("../php_jit/src"),
        manifest.join("../php_ir/src"),
        manifest.join("../php_runtime/src"),
    ];
    let mut files = Vec::new();
    for input in &inputs {
        println!("cargo:rerun-if-changed={}", input.display());
        collect_rust_files(input, &mut files);
    }
    files.sort();
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for path in files {
        let relative = path
            .strip_prefix(crates_root)
            .expect("native build identity input is under crates/");
        hash = hash_bytes(hash, relative.to_string_lossy().as_bytes());
        hash = hash_bytes(
            hash,
            &fs::read(&path).expect("read native build identity input"),
        );
    }
    println!("cargo:rustc-env=PHRUST_AUTO_BUILD_ID=native-source-v1-{hash:016x}");
}

fn collect_rust_files(path: &Path, files: &mut Vec<PathBuf>) {
    let mut entries = fs::read_dir(path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
        .map(|entry| entry.expect("read native build identity directory entry"))
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

fn hash_bytes(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
