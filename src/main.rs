mod aur_helper;
mod libalpm;
mod loader;
mod lock;
mod settings;

use std::{
    collections::{HashMap, HashSet},
    io::{self, BufRead},
    process,
    time::Instant,
};

use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use which::which;

use crate::{
    aur_helper::launch_reinstall_cmd,
    libalpm::{
        get_aur_packages, get_local_packages, get_required_by_of_package,
        get_so_dependencies_of_pkg, get_so_files_of_pkg,
    },
    lock::LockFile,
    settings::{Settings, load_settings, save_settings},
};

fn on_update() {
    let _lock = match LockFile::try_to_acquire("aur-check-rebuild".to_string()) {
        Ok(_lock) => _lock,
        Err(_) => {
            process::exit(0);
        }
    };

    let settings = load_settings().unwrap();

    let stdin = io::stdin();
    let mut updated_pkgs: Vec<String> = stdin.lock().lines().map(Result::unwrap).collect();
    println!("Triggered by {} packages", updated_pkgs.len());

    let mut packages_to_rebuild: HashSet<String> = HashSet::new();

    let local_packages = get_local_packages();
    let aur_packages = get_aur_packages();

    println!("Installed packages:");
    println!("  {} from repository", local_packages.len());
    println!("  {} from AUR", aur_packages.len());

    println!("Looking for dependant packages...");
    let t0 = Instant::now();
    let mut acc_so_to_pkg: HashMap<String, String> = HashMap::new();
    loop {
        let so_files_updated: HashMap<String, HashSet<String>> = updated_pkgs
            .par_iter()
            .map(|pkg| {
                let files: HashSet<String> = get_so_files_of_pkg(pkg).into_iter().collect();
                (pkg.clone(), files)
            })
            .collect();

        let so_to_pkg: HashMap<String, String> = so_files_updated
            .into_iter()
            .flat_map(|(pkg, files)| files.into_iter().map(move |f| (f.clone(), pkg.clone())))
            .collect();
        so_to_pkg.clone().into_iter().for_each(|f| {
            acc_so_to_pkg.entry(f.0.clone()).insert_entry(f.1.clone());
        });

        let dependant_packages: HashSet<String> = updated_pkgs
            .par_iter()
            .flat_map(|pkg| get_required_by_of_package(pkg))
            .collect();

        let dependant_aur_pkgs: Vec<String> = aur_packages
            .par_iter()
            .filter(|aur| dependant_packages.contains(*aur))
            .cloned()
            .collect();

        let added_pkgs: Vec<String> = dependant_aur_pkgs
            .into_par_iter()
            .filter(|aur| {
                get_so_dependencies_of_pkg(aur)
                    .into_iter()
                    .any(|dep| so_to_pkg.contains_key(&dep))
            })
            .collect();

        if !settings.scan.recursive || added_pkgs.is_empty() {
            break;
        }

        for pkg in &added_pkgs {
            updated_pkgs.push(pkg.clone());
            packages_to_rebuild.insert(pkg.clone());
        }

        updated_pkgs = added_pkgs.into_iter().collect();
    }
    let mut packages_to_rebuild: Vec<String> = packages_to_rebuild.into_iter().collect();
    packages_to_rebuild.sort();

    let dep_map: HashMap<_, _> = packages_to_rebuild
        .par_iter()
        .filter_map(|aur| {
            let deps: HashSet<String> = get_so_dependencies_of_pkg(aur)
                .into_iter()
                .filter_map(|dep| acc_so_to_pkg.get(&dep).cloned())
                .filter(|d| d != aur)
                .collect();

            (!deps.is_empty()).then(|| (aur.clone(), deps))
        })
        .collect();
    println!("  Scan finished after {:.3}", t0.elapsed().as_secs_f64());

    println!("Packages to rebuild: {}", &packages_to_rebuild.len());
    for pkg in &packages_to_rebuild {
        let mut deps = dep_map
            .get(pkg)
            .unwrap()
            .into_iter()
            .cloned()
            .collect::<Vec<String>>();
        deps.sort();
        println!(
            "  \x1b[1;37m{}\x1b[0m: \x1b[90m{}\x1b[0m",
            pkg,
            deps.join(" ")
        );
    }

    let mut helper = None;
    for hlp in ["paru", "yay"] {
        if which(hlp).is_ok() {
            helper = Some(hlp);
            break;
        }
    }

    if packages_to_rebuild.len() > 0 {
        launch_reinstall_cmd(helper.unwrap(), settings, packages_to_rebuild);
    }
}

fn on_settings() {
    let _ = save_settings(&Settings::default());
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.contains(&"--settings".to_string()) {
        on_settings();
    } else {
        on_update();
    }
}
