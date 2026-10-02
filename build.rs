use std::{fs, path::Path};

// Server and WASM are built separately; hashing the shared sources gives both the same id.
fn main() {
    let inputs = ["src", "style", "Cargo.toml"];
    let mut files = Vec::new();
    for input in inputs {
        println!("cargo:rerun-if-changed={input}");
        collect(Path::new(input), &mut files);
    }
    files.sort();

    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |bytes: &[u8]| {
        for &b in bytes {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    };
    for file in &files {
        feed(file.as_bytes());
        let content = fs::read(file).unwrap_or_default();
        // Ignore CRLF vs LF so checkouts on different OSes agree.
        let normalized: Vec<u8> = content.into_iter().filter(|&b| b != b'\r').collect();
        feed(&normalized);
    }

    let version = std::env::var("CARGO_PKG_VERSION").unwrap();
    println!(
        "cargo:rustc-env=BUILD_ID={version}+{:08x}",
        hash as u32 ^ (hash >> 32) as u32
    );
}

fn collect(path: &Path, out: &mut Vec<String>) {
    if path.is_dir() {
        let Ok(entries) = fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            collect(&entry.path(), out);
        }
    } else if path.is_file() {
        out.push(path.to_string_lossy().replace('\\', "/"));
    }
}
