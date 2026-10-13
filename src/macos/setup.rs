//! One-time setup of this machine; each step skips itself once done.

use std::{fmt, path::PathBuf, process::Command};

use super::Error;

/// A one-time change to this machine.
pub trait Step: fmt::Display {
    fn is_done(&self) -> Result<bool, Error>;
    fn run(&self) -> Result<(), Error>;
}

/// A self-signed code-signing certificate in the login keychain, trusted for code signing; it signs
/// the app, so the app's permissions survive rebuilds.
pub struct Certificate {
    pub name: String,
}

/// Runs each step that is not yet done, reporting each.
pub fn setup(steps: &[&dyn Step]) -> Result<(), Error> {
    for step in steps {
        if step.is_done()? {
            println!("{step}: done");
        } else {
            step.run()?;
            println!("{step}: set up");
        }
    }
    Ok(())
}

impl fmt::Display for Certificate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "code-signing certificate \"{}\"", self.name)
    }
}

impl Step for Certificate {
    fn is_done(&self) -> Result<bool, Error> {
        let output = Command::new("/usr/bin/security")
            .args(["find-identity", "-v", "-p", "codesigning"])
            .arg(keychain())
            .output()?;
        let identities = String::from_utf8_lossy(&output.stdout);
        Ok(identities.contains(&format!("\"{}\"", self.name)))
    }

    /// `sudo` asks for the password to trust the certificate.
    fn run(&self) -> Result<(), Error> {
        let dir = tempfile::tempdir()?;
        let key = dir.path().join("key.pem");
        let certificate = dir.path().join("certificate.pem");
        let identity = dir.path().join("identity.p12");
        run(Command::new("/usr/bin/openssl")
            .args([
                "req", "-x509", "-newkey", "rsa:3072", "-nodes", "-days", "3650",
            ])
            .args(["-subj", &format!("/CN={}", self.name)])
            .args(["-addext", "keyUsage=critical,digitalSignature"])
            .args(["-addext", "extendedKeyUsage=critical,codeSigning"])
            .arg("-keyout")
            .arg(&key)
            .arg("-out")
            .arg(&certificate))?;
        run(Command::new("/usr/bin/openssl")
            .args(["pkcs12", "-export", "-passout", "pass:import", "-inkey"])
            .arg(&key)
            .arg("-in")
            .arg(&certificate)
            .arg("-out")
            .arg(&identity))?;
        run(Command::new("/usr/bin/security")
            .arg("import")
            .arg(&identity)
            .arg("-k")
            .arg(keychain())
            .args(["-P", "import", "-T", "/usr/bin/codesign"]))?;
        run(Command::new("/usr/bin/sudo")
            .args([
                "/usr/bin/security",
                "add-trusted-cert",
                "-d",
                "-r",
                "trustRoot",
            ])
            .args(["-p", "codeSign", "-k"])
            .arg(keychain())
            .arg(&certificate))
    }
}

/// The login keychain.
fn keychain() -> PathBuf {
    etcetera::home_dir()
        .unwrap_or_else(|_| PathBuf::from("~"))
        .join("Library/Keychains/login.keychain-db")
}

/// Runs `command`, failing when it does.
fn run(command: &mut Command) -> Result<(), Error> {
    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::Setup {
            program: command.get_program().into(),
            status,
        })
    }
}
