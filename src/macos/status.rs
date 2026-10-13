//! The installed app, its process and its signature.

use std::{
    fmt,
    path::{Path, PathBuf},
    process::Command,
};

use super::setup::Permission;
use libproc::{
    libproc::proc_pid::pidpath,
    processes::{ProcFilter, pids_by_type},
};

/// The app at ~/Applications/Accessibility.app, where the home-manager module installs it.
pub struct Status {
    pid: Option<u32>,
    bundle: PathBuf,
    signer: Option<String>,
    permission: Option<Permission>,
}

impl Status {
    pub fn read() -> Self {
        let bundle = etcetera::home_dir()
            .unwrap_or_default()
            .join("Applications/Accessibility.app");
        let executable = bundle.join("Contents/MacOS/accessibility");
        let pid = pids_by_type(ProcFilter::All)
            .unwrap_or_default()
            .into_iter()
            .find(|&pid| {
                i32::try_from(pid)
                    .ok()
                    .and_then(|pid| pidpath(pid).ok())
                    .is_some_and(|path| Path::new(&path) == executable)
            });
        let signer = Command::new("/usr/bin/codesign")
            .args(["-d", "-vv"])
            .arg(&bundle)
            .output()
            .ok()
            .and_then(|output| {
                String::from_utf8_lossy(&output.stderr)
                    .lines()
                    .find_map(|line| line.strip_prefix("Authority="))
                    .map(ToOwned::to_owned)
            });
        Self {
            pid,
            bundle,
            signer,
            permission: Permission::read(),
        }
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.pid {
            Some(pid) => writeln!(f, "running:    pid {pid}")?,
            None => writeln!(f, "running:    no")?,
        }
        writeln!(f, "app:        {}", self.bundle.display())?;
        match &self.signer {
            Some(signer) => writeln!(f, "signed by:  {signer}")?,
            None => writeln!(f, "signed by:  no certificate")?,
        }
        match self.permission {
            Some(permission) => write!(f, "permission: {permission}"),
            None => write!(f, "permission: unknown"),
        }
    }
}
