use std::{
    env, fs,
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::Path,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::{
    libalpm::{DB_LOCK_FILE, PACMAN_CACHE, PacmanPackage, load_package_from_file},
    settings::Settings,
};

pub fn get_package_info(helper: &str, pkg: &str) -> PacmanPackage {
    let output = Command::new(helper)
        .env("LANG", "C")
        .args(["-Qi", pkg])
        .output()
        .expect("failed to get package information");

    let mut name: Option<String> = None;
    let mut version: Option<String> = None;

    String::from_utf8_lossy(&output.stdout)
        .to_string()
        .lines()
        .for_each(|line| {
            if line.starts_with("Name") {
                name = Some(line.splitn(2, ":").nth(1).unwrap().trim().to_string());
            } else if line.starts_with("Version") {
                version = Some(line.splitn(2, ":").nth(1).unwrap().trim().to_string());
            }
        });

    let desc_path = format!(
        "{}{}-{}/desc",
        PACMAN_CACHE,
        name.unwrap(),
        version.unwrap()
    );

    load_package_from_file(&desc_path)
}

pub fn launch_reinstall_cmd(helper: &str, settings: Settings, package_lists: Vec<String>) {
    if settings.rebuild.automatic {
        let mut db_lock_exists = false;
        if fs::exists(DB_LOCK_FILE).unwrap() {
            db_lock_exists = true;
            fs::remove_file(DB_LOCK_FILE).unwrap();
        }

        let helper = Arc::new(helper.to_string());
        let pkgs_arc = Arc::new(package_lists.clone());
        let finished = Arc::new(Mutex::new(Vec::<String>::new()));
        let stop_flag = Arc::new(Mutex::new(false));

        {
            let helper = Arc::clone(&helper);
            let pkgs = Arc::clone(&pkgs_arc);
            let finished = Arc::clone(&finished);
            let stop_flag = Arc::clone(&stop_flag);

            thread::spawn(move || {
                let prt = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;

                while !*stop_flag.lock().unwrap() {
                    while Path::new(DB_LOCK_FILE).exists() {
                        thread::sleep(Duration::from_millis(100));
                    }

                    for p in pkgs.iter() {
                        let mut fin = finished.lock().unwrap();
                        if fin.contains(p) {
                            continue;
                        }

                        let pkg = get_package_info(&helper, p);
                        if prt < pkg.install_date {
                            fin.push(p.clone());
                            println!("  [{}/{}] Rebuilt {}", fin.len(), pkgs.len(), p);
                        }
                    }

                    thread::sleep(Duration::from_millis(500));
                }
            });
        }

        let helper_command = format!(
            "{} -S --aur --mflags \"--skippgpcheck --skipchecksums --nocheck\" --sudoloop --noconfirm {}",
            helper,
            pkgs_arc.join(" ")
        );

        let echo_command = format!(
            "echo \"Rebuilding packages:\n  {}\n\"",
            pkgs_arc.join("\n  ")
        );
        let full_cmd =
            format!("{echo_command} && {helper_command}; read -p \"Press enter to exit...\"");

        let mut final_cmd = Vec::new();

        let user = env::var("SUDO_USER").unwrap_or_else(|_| "root".to_string());
        if unsafe { libc::geteuid() } == 0 {
            final_cmd.push("sudo");
            final_cmd.push("-u");
            final_cmd.push(&user);
            final_cmd.push("--");
        }
        final_cmd.push("alacritty");
        final_cmd.push("-o");
        final_cmd.push("window.opacity=1.0");
        final_cmd.push("-T");
        final_cmd.push("Rebuilding AUR packages");
        final_cmd.push("-e");
        final_cmd.push("bash");
        final_cmd.push("-c");
        final_cmd.push(&full_cmd);

        println!("Waiting for rebuild process...");
        let start = Instant::now();

        let mut cmd = Command::new(final_cmd[0]);
        cmd.args(&final_cmd[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0);

        let status = cmd.status();
        *stop_flag.lock().unwrap() = true;
        println!(
            "Finished after {:.3}s with status {:?}",
            start.elapsed().as_secs_f64(),
            status.unwrap().code().unwrap()
        );

        if db_lock_exists {
            let _ = fs::OpenOptions::new()
                .create(true)
                .write(true)
                .open(DB_LOCK_FILE);

            let _ = fs::set_permissions(DB_LOCK_FILE, fs::Permissions::from_mode(0o000));
        }
        let finished = finished.lock().unwrap();
        let pending: Vec<_> = pkgs_arc
            .iter()
            .filter(|p| !finished.contains(p))
            .cloned()
            .collect();

        if pending.is_empty() {
            println!("  Packages rebuilt successfully");
        } else {
            println!("  Process finished with {} pending packages", pending.len());
            println!("    Run the following command to perform rebuild:");
            println!("      {helper} -S --aur {}", pending.join(" "));
        }
    } else {
        println!("Run the following command to perform rebuild:");
        println!("  {helper} -S --aur {}", package_lists.join(" "));
    }
}
