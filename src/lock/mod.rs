use std::{ffi::OsStr, fs, path::PathBuf, process};

use sysinfo::{Pid, ProcessesToUpdate, System};

pub struct LockFile {
    path: PathBuf,
}

impl LockFile {
    pub fn try_to_acquire(path: String) -> std::io::Result<Self> {
        let path = std::path::PathBuf::from(format!("/tmp/{}.lock", path));

        if fs::exists(&path).unwrap() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(pid_num) = content.trim().parse::<usize>() {
                    let mut sys = System::new_all();
                    sys.refresh_processes(ProcessesToUpdate::All, true);

                    let current_file = std::env::args().next().unwrap_or_default();

                    if let Some(process) = sys.process(Pid::from(pid_num)) {
                        if process
                            .cmd()
                            .iter()
                            .any(|arg| arg == OsStr::new(&current_file))
                        {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::AlreadyExists,
                                "Lock already acquired by other instance",
                            ));
                        }
                    }
                }
            }
        }
        std::fs::write(&path, process::id().to_string())?;

        Ok(Self { path })
    }
}

impl Drop for LockFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
