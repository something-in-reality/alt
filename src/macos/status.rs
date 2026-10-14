//! The installed app, its process, its signature and its Accessibility permission, each read
//! from the system.

use std::{
    fmt,
    path::{Path, PathBuf},
    process::Command,
};

use libproc::{
    libproc::proc_pid::pidpath,
    processes::{ProcFilter, pids_by_type},
};
use objc2_core_graphics::{CGError, CGGetEventTapList};

/// The app at ~/Applications/Alt.app, where the home-manager module installs it.
pub struct Status {
    pid: Option<u32>,
    bundle: PathBuf,
    signer: Option<String>,
}

impl Status {
    pub fn read() -> Self {
        let bundle = bundle();
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
            pid: daemon(),
            bundle,
            signer,
        }
    }
}

fn bundle() -> PathBuf {
    etcetera::home_dir()
        .unwrap_or_default()
        .join("Applications/Alt.app")
}

/// The running daemon: the process of the installed app's executable.
pub fn daemon() -> Option<u32> {
    let executable = bundle().join("Contents/MacOS/alt");
    pids_by_type(ProcFilter::All)
        .unwrap_or_default()
        .into_iter()
        .find(|&pid| {
            i32::try_from(pid)
                .ok()
                .and_then(|pid| pidpath(pid).ok())
                .is_some_and(|path| Path::new(&path) == executable)
        })
}

/// Whether `pid` holds an enabled event tap; macOS installs a tap that changes events only for a
/// process with the Accessibility permission.
pub fn trusted(pid: u32) -> bool {
    let mut count = 0;
    if unsafe { CGGetEventTapList(0, std::ptr::null_mut(), &mut count) } != CGError::Success {
        return false;
    }
    let mut taps = Vec::with_capacity(count as usize);
    if unsafe { CGGetEventTapList(count, taps.as_mut_ptr(), &mut count) } != CGError::Success {
        return false;
    }
    // SAFETY: CGGetEventTapList initialised `count` entries, at most the capacity.
    unsafe { taps.set_len(count as usize) };
    taps.iter()
        .any(|tap| u32::try_from(tap.tappingProcess) == Ok(pid) && tap.enabled)
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
        match self.pid {
            Some(pid) if trusted(pid) => write!(f, "permission: granted"),
            Some(_) => write!(f, "permission: waiting"),
            None => write!(f, "permission: unknown"),
        }
    }
}
