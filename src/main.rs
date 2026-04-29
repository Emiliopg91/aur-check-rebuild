use std::{
    collections::{HashMap, HashSet},
    io::{self, BufRead},
};

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use crate::{
    cache::{Cache, CacheEntry},
    libalpm::{
        LOCALDB, get_depends_of_package, get_package_info, get_so_dependencies_of_pkg,
        get_so_files_of_pkg,
    },
};

mod cache;
mod libalpm;
mod loader;

fn read_from_stdin() -> Vec<String> {
    let stdin = io::stdin();
    let input = stdin.lock().lines().map(Result::unwrap).collect();
    return input;
}

fn on_cache() {
    println!("Building dependency hierarchy...");
    let dependencies = LOCALDB
        .par_iter()
        .map(|(name, _)| (name.clone(), get_depends_of_package(name)))
        .collect::<HashMap<String, Vec<String>>>();
    println!("Listing shared libraries...");
    let so_files = LOCALDB
        .par_iter()
        .map(|(name, _)| (name.clone(), get_so_files_of_pkg(name)))
        .collect::<HashMap<String, Vec<String>>>();
    println!("Building libraries dependency hierarchy...");
    let so_dependencies = LOCALDB
        .par_iter()
        .map(|(name, _)| (name.clone(), get_so_dependencies_of_pkg(name)))
        .collect::<HashMap<String, HashMap<String, Vec<String>>>>();

    let mut data: HashMap<String, CacheEntry> = HashMap::new();
    for pkg in LOCALDB.keys() {
        let mut dependencies = dependencies.get(pkg).unwrap().clone();
        let mut so_files = so_files.get(pkg).unwrap().clone();
        let so_dependencies = so_dependencies.get(pkg).unwrap().clone();
        let db = get_package_info(pkg).db;

        dependencies.sort();
        so_files.sort();

        data.insert(
            pkg.clone(),
            CacheEntry {
                dependencies: dependencies.clone(),
                db,
                so_files: so_files.clone(),
                so_dependencies: so_dependencies.clone(),
            },
        );
    }

    println!("Saving cache...");
    Cache { data }.store().unwrap();
}

fn on_post_install() {
    let installed_packages = read_from_stdin();
    if installed_packages.len() == 0 {
        return;
    }

    println!("Loading cache...");
    let mut cache = Cache::load().unwrap();

    println!("Updating cache...");
    for pkg in installed_packages {
        let dependencies = get_depends_of_package(&pkg);
        let so_files = get_so_files_of_pkg(&pkg);
        let so_dependencies = get_so_dependencies_of_pkg(&pkg);
        let db = get_package_info(&pkg).db;

        cache.data.entry(pkg).insert_entry(CacheEntry {
            dependencies,
            db,
            so_files,
            so_dependencies,
        });
    }
    cache.store().unwrap();
}

fn on_post_remove() {
    let removed_packages = read_from_stdin();
    if removed_packages.len() == 0 {
        return;
    }

    println!("Loading cache...");
    let mut cache = Cache::load().unwrap();

    println!("Updating cache...");
    for pkg in removed_packages {
        cache.data.remove(&pkg);
    }
    cache.store().unwrap();
}

fn on_post_upgrade() {
    let updated_packages = read_from_stdin();
    if updated_packages.len() == 0 {
        return;
    }

    println!("Loading cache...");
    let old_cache = Cache::load().unwrap();
    let mut new_cache = old_cache.clone();

    println!("Updating cache...");
    let mut acc_so_files: HashMap<String, String> = HashMap::new();
    for pkg in &updated_packages {
        let dependencies = get_depends_of_package(&pkg);
        let so_files = get_so_files_of_pkg(&pkg);
        let so_dependencies = get_so_dependencies_of_pkg(&pkg);
        let db = get_package_info(pkg).db;

        new_cache.data.entry(pkg.clone()).insert_entry(CacheEntry {
            dependencies,
            db,
            so_files,
            so_dependencies,
        });

        let so_of_pkg = get_so_files_of_pkg(&pkg);
        for so_file in &so_of_pkg {
            acc_so_files
                .entry(so_file.clone())
                .insert_entry(pkg.clone());
        }
        if let Some(vec) = old_cache.data.get(pkg) {
            let set_a: HashSet<String> = vec.so_files.iter().cloned().collect();
            let set_b: HashSet<String> = so_of_pkg.iter().cloned().collect();
            set_a.difference(&set_b).for_each(|f| {
                acc_so_files.entry(f.clone()).insert_entry(pkg.clone());
            });
        }

        let mut execs_to_scan: HashMap<String, Vec<String>> = HashMap::new();
        for (pkg_name, entry) in &new_cache.data {
            let mut execs = Vec::new();

            for (exec, so_deps) in &entry.so_dependencies {
                if so_deps.iter().any(|f| acc_so_files.contains_key(f)) {
                    execs.push(exec.clone())
                }
            }

            if execs.len() > 0 {
                execs_to_scan.entry(pkg_name.clone()).insert_entry(execs);
            }
        }

        for (pkg_name, execs) in &execs_to_scan {
            let so_deps_of_pkg = get_so_dependencies_of_pkg(pkg_name);

            if let Some(entry) = new_cache.data.get_mut(pkg_name) {
                for exec in execs {
                    if let Some(deps) = so_deps_of_pkg.get(exec) {
                        entry.so_dependencies.insert(exec.clone(), deps.clone());
                    }
                }
            }
        }
    }
    new_cache.clone().store().unwrap();

    println!("Collecting shared objects...");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.contains(&"--cache".to_string()) {
        on_cache();
    } else if args.contains(&"--post-install".to_string()) {
        on_post_install();
    } else if args.contains(&"--post-remove".to_string()) {
        on_post_remove();
    } else if args.contains(&"--post-upgrade".to_string()) {
        on_post_upgrade();
    }
}
