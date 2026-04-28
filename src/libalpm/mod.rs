use cached::proc_macro::cached;
use glob::glob;
use once_cell::sync::Lazy;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use regex::Regex;

use std::{
    collections::{HashMap, HashSet},
    fs,
};

use crate::loader::get_needed_shared_objects;

static SO_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"lib[^/]+\.so(\.[0-9]+)*$").unwrap());
pub const PACMAN_CACHE: &str = "/var/lib/pacman/local/";

pub const DB_LOCK_FILE: &str = "/var/lib/pacman/db.lck";

#[derive(Debug, Clone)]
pub struct PacmanDesc {
    pub fields: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct PacmanPackage {
    pub name: String,
    pub version: String,
    pub depends: Vec<String>,
    pub db: String,
    pub install_date: i64,
}

pub fn parse_pacman_desc_file(path: &str) -> PacmanDesc {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => {
            return PacmanDesc {
                fields: std::collections::HashMap::new(),
            };
        }
    };

    let mut map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    let mut current_key: Option<String> = None;

    for line in content.lines() {
        let line = line.trim();

        if line.starts_with('%') && line.ends_with('%') {
            current_key = Some(line.trim_matches('%').to_string());
            continue;
        }

        if let Some(key) = &current_key {
            if !line.is_empty() {
                map.entry(key.clone()).or_default().push(line.to_string());
            }
        }
    }

    PacmanDesc { fields: map }
}

pub fn load_package_from_file(file: &str) -> PacmanPackage {
    let desc = parse_pacman_desc_file(&file);

    let name = desc
        .fields
        .get("NAME")
        .and_then(|v| v.first())
        .cloned()
        .unwrap_or_else(|| "unknown".to_string());

    let version = desc
        .fields
        .get("VERSION")
        .and_then(|v| v.first())
        .cloned()
        .unwrap_or_else(|| "unknown".to_string());

    let install_date = desc
        .fields
        .get("INSTALLDATE")
        .and_then(|v| v.first())
        .cloned()
        .map(|s| s.parse::<i64>().unwrap())
        .unwrap();

    let db = desc
        .fields
        .get("INSTALLED_DB")
        .and_then(|v| v.first())
        .cloned()
        .unwrap_or_else(|| "unknown".to_string());

    let depends = desc.fields.get("DEPENDS").cloned().unwrap_or_default();

    PacmanPackage {
        name,
        version,
        depends,
        db,
        install_date,
    }
}

#[cached]
pub fn load_localdb_packages() -> HashMap<String, PacmanPackage> {
    let mut data: HashMap<String, PacmanPackage> = HashMap::new();
    glob(&format!("{}*", &PACMAN_CACHE))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|p| p.is_dir())
        .for_each(|p| {
            let desc_path = p.display().to_string() + "/desc";
            let package = load_package_from_file(&desc_path);
            data.entry(package.name.clone()).insert_entry(package);
        });
    return data;
}

static LOCALDB: Lazy<HashMap<String, PacmanPackage>> = Lazy::new(|| load_localdb_packages());

#[cached]
pub fn get_local_packages() -> Vec<String> {
    LOCALDB.keys().map(|s| s.to_string()).collect()
}

pub fn get_aur_packages() -> Vec<String> {
    LOCALDB
        .iter()
        .filter(|p| p.1.db == "unknown")
        .map(|p| p.0.to_string())
        .collect::<Vec<String>>()
}

#[cached(key = "String", convert = r#"{ pkg.to_string() }"#)]
pub fn get_depends_of_package(pkg: &str) -> Vec<String> {
    get_package_info(pkg).depends
}

#[cached(key = "String", convert = r#"{ pkg.to_string() }"#)]
pub fn get_required_by_of_package(pkg: &str) -> Vec<String> {
    let mut required = Vec::new();
    let keys: Vec<String> = LOCALDB.keys().cloned().collect();
    for key in keys {
        let deps = LOCALDB.get(&key).unwrap().depends.clone();
        if deps.contains(&(pkg.to_string())) {
            required.push(key);
        }
    }
    return required;
}

#[cached(key = "String", convert = r#"{ pkg.to_string() }"#)]
pub fn get_files_of_package(pkg: &str) -> Vec<String> {
    let p = LOCALDB.get(pkg).unwrap();
    let files_path = format!("{}{}-{}/files", PACMAN_CACHE, pkg, p.version);
    let mut files = Vec::new();
    if fs::exists(&files_path).unwrap_or(false) {
        let content = fs::read_to_string(&files_path).unwrap_or("".to_string());
        if content != "" {
            let mut iter = content.lines();

            iter.by_ref()
                .take_while(|l| *l != "%BACKUP%")
                .skip(1)
                .map(|f| format!("/{}", f))
                .filter(|f| fs::metadata(f).map(|m| m.is_file()).unwrap_or(false))
                .for_each(|f| files.push(f));
        }
    }
    return files;
}

#[cached(key = "String", convert = r#"{ pkg.to_string() }"#)]
pub fn get_so_files_of_pkg(pkg: &str) -> Vec<String> {
    get_files_of_package(pkg)
        .iter()
        .filter(|path| SO_PATTERN.is_match(path))
        .map(|s| s.to_string())
        .collect()
}

#[cached(key = "String", convert = r#"{ pkg.to_string() }"#)]
pub fn get_so_dependencies_of_pkg(pkg: &str) -> Vec<String> {
    get_files_of_package(pkg)
        .par_iter()
        .flat_map(|f| get_needed_shared_objects(f))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect()
}

#[cached(key = "String", convert = r#"{ pkg.to_string() }"#)]
pub fn get_package_info(pkg: &str) -> PacmanPackage {
    LOCALDB.get(pkg).unwrap().clone()
}
