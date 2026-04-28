use std::{collections::HashMap, fs, process::Command};

use cached::proc_macro::cached;
use once_cell::sync::Lazy;

static LD_CACHE: Lazy<(HashMap<String, String>, HashMap<String, String>)> = Lazy::new(|| {
    let output = Command::new("ldconfig")
        .arg("-p")
        .output()
        .expect("ldconfig failed")
        .stdout;

    let text = String::from_utf8_lossy(&output);

    let mut lib32 = HashMap::new();
    let mut lib64 = HashMap::new();

    for line in text.lines().skip(1) {
        let line = line.trim();

        let mut parts = line.split(" => ");
        let left = parts.next();
        let path = parts.next();

        let (Some(left), Some(path)) = (left, path) else {
            continue;
        };

        let name = left.split(" (").next().unwrap();

        if left.contains("x86-64") {
            lib64.insert(name.to_string(), path.to_string());
        } else {
            lib32.insert(name.to_string(), path.to_string());
        }
    }

    (lib32, lib64)
});

#[cached(key = "String", convert = r#"{ path.to_string() }"#)]
fn is_elf(path: &str) -> Option<u8> {
    let buf = fs::read(path).ok()?;

    let elf_class = *buf.get(4)?;
    match (buf.get(0..4)?, elf_class) {
        (b"\x7fELF", 1) => Some(32),
        (b"\x7fELF", 2) => Some(64),
        _ => None,
    }
}

pub fn get_needed_shared_objects(path: &str) -> Vec<String> {
    let arch = match is_elf(path) {
        Some(a) => a,
        None => return Vec::new(),
    };

    let buffer = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return Vec::new(),
    };

    let elf = match goblin::elf::Elf::parse(&buffer) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let cache = match arch {
        32 => &LD_CACHE.0,
        64 => &LD_CACHE.1,
        _ => return Vec::new(),
    };

    let Some(dynamic) = &elf.dynamic else {
        return Vec::new();
    };

    let mut needed = Vec::new();

    for entry in &dynamic.dyns {
        if entry.d_tag != goblin::elf::dynamic::DT_NEEDED {
            continue;
        }

        if let Some(name) = elf.dynstrtab.get_at(entry.d_val as usize) {
            if let Some(p) = cache.get(name) {
                needed.push(p.clone());
            }
        }
    }

    needed
}
