//! The running app and its signature.

use std::{fmt, path::PathBuf, process::Command};

use objc2_app_kit::NSRunningApplication;
use objc2_foundation::NSString;

use super::IDENTIFIER;

/// The app as the system sees it.
pub struct Status {
    pid: Option<i32>,
    bundle: PathBuf,
    signer: Option<String>,
}

impl Status {
    pub fn read() -> Self {
        let running = NSRunningApplication::runningApplicationsWithBundleIdentifier(
            &NSString::from_str(IDENTIFIER),
        )
        .firstObject();
        let bundle = running
            .as_deref()
            .and_then(|app| app.bundleURL())
            .and_then(|url| url.path())
            .map(|path| PathBuf::from(path.to_string()))
            .unwrap_or_else(|| {
                etcetera::home_dir()
                    .unwrap_or_default()
                    .join("Applications/Accessibility.app")
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
            pid: running.map(|app| app.processIdentifier()),
            bundle,
            signer,
        }
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.pid {
            Some(pid) => writeln!(f, "running:   pid {pid}")?,
            None => writeln!(f, "running:   no")?,
        }
        writeln!(f, "app:       {}", self.bundle.display())?;
        match &self.signer {
            Some(signer) => write!(f, "signed by: {signer}"),
            None => write!(f, "signed by: no certificate"),
        }
    }
}
