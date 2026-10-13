//! `accessibility setup`: a checklist of the machine's one-time steps, each with its current
//! status, run in order once selected.

use std::{
    fmt, fs,
    path::{Path, PathBuf},
    process::Command,
    str::FromStr,
    thread,
    time::Duration,
};

use anyhow::{Context, bail};
use etcetera::BaseStrategy;
use inquire::MultiSelect;

/// Whether the daemon holds its Accessibility permission, as it last recorded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    Waiting,
    Granted,
}

/// A half of the Glove80, which flashes from its own bootloader volume.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Half {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Permission,
    Glove80(Half),
}

/// A step and whether it is done, as the checklist shows it.
struct Row {
    step: Step,
    done: bool,
}

const POLL: Duration = Duration::from_millis(250);

impl Permission {
    fn path() -> Option<PathBuf> {
        state_dir().map(|dir| dir.join("permission"))
    }

    pub fn read() -> Option<Self> {
        fs::read_to_string(Self::path()?).ok()?.trim().parse().ok()
    }

    /// Records the permission for `setup` and `status`; a failure only logs.
    pub fn record(self) {
        let written = Self::path()
            .context("no state directory")
            .and_then(|path| write(&path, &self.to_string()));
        if let Err(error) = written {
            tracing::warn!(error = %format_args!("{error:#}"), "recording the permission");
        }
    }
}

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Waiting => "waiting",
            Self::Granted => "granted",
        })
    }
}

impl FromStr for Permission {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "waiting" => Ok(Self::Waiting),
            "granted" => Ok(Self::Granted),
            _ => Err(format!("unknown permission `{s}`")),
        }
    }
}

impl Half {
    /// The bootloader volume this half mounts.
    fn volume(self) -> &'static Path {
        Path::new(match self {
            Self::Left => "/Volumes/GLV80LHBOOT",
            Self::Right => "/Volumes/GLV80RHBOOT",
        })
    }

    /// The Magic-layer key that starts this half's bootloader.
    fn bootloader_key(self) -> &'static str {
        match self {
            Self::Left => "Esc",
            Self::Right => "'",
        }
    }

    /// The firmware hash this half was last flashed with.
    fn flashed(self) -> Option<PathBuf> {
        state_dir().map(|dir| dir.join(format!("glove80-{self}")))
    }
}

impl fmt::Display for Half {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Left => "left",
            Self::Right => "right",
        })
    }
}

impl Step {
    const ALL: [Self; 3] = [
        Self::Permission,
        Self::Glove80(Half::Left),
        Self::Glove80(Half::Right),
    ];

    fn done(self, firmware: Option<&str>) -> bool {
        match self {
            Self::Permission => Permission::read() == Some(Permission::Granted),
            Self::Glove80(half) => firmware.is_some_and(|hash| {
                half.flashed()
                    .and_then(|path| fs::read_to_string(path).ok())
                    .is_some_and(|flashed| flashed.trim() == hash)
            }),
        }
    }

    fn run(self, firmware: &Path) -> anyhow::Result<()> {
        match self {
            Self::Permission => {
                Command::new("/usr/bin/open")
                    .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
                    .status()?;
                println!("Turn on Accessibility; waiting for the app to start with it.");
                while Permission::read() != Some(Permission::Granted) {
                    thread::sleep(POLL);
                }
                Ok(())
            }
            Self::Glove80(half) => flash(half, firmware),
        }
    }
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Permission => f.write_str("Accessibility permission"),
            Self::Glove80(half) => write!(f, "Glove80 firmware, {half} half"),
        }
    }
}

impl fmt::Display for Row {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", if self.done { "✓" } else { "·" }, self.step)
    }
}

/// Shows each step's status, preselects those not done, and runs the selection in order.
pub fn setup() -> anyhow::Result<()> {
    let firmware = data_dir().context("no data directory")?.join("glove80.uf2");
    let hash = fs::read(&firmware)
        .ok()
        .map(|bytes| blake3::hash(&bytes).to_hex().to_string());
    let rows: Vec<Row> = Step::ALL
        .into_iter()
        .map(|step| Row {
            step,
            done: step.done(hash.as_deref()),
        })
        .collect();
    let pending: Vec<usize> = (0..rows.len()).filter(|&i| !rows[i].done).collect();
    let selected = MultiSelect::new("Set up this machine:", rows)
        .with_default(&pending)
        .prompt()?;
    for Row { step, .. } in selected {
        println!("{step}");
        step.run(&firmware)?;
    }
    Ok(())
}

/// Copies `firmware` to `half`'s bootloader volume and records its hash once the half restarts.
fn flash(half: Half, firmware: &Path) -> anyhow::Result<()> {
    let bytes = fs::read(firmware).with_context(|| {
        format!(
            "reading {}; enable services.accessibility.glove80",
            firmware.display()
        )
    })?;
    let volume = half.volume();
    println!(
        "Hold Magic and press {} to start the {half} half's bootloader.",
        half.bootloader_key()
    );
    while !volume.exists() {
        thread::sleep(POLL);
    }
    // The volume appears before it accepts writes, so the copy retries while it stays mounted. The
    // half restarts once it has written the last block, which can end the copy with an error; the
    // restart is the success.
    let mut copied = fs::write(volume.join("glove80.uf2"), &bytes);
    for _ in 0..20 {
        if copied.is_ok() || !volume.exists() {
            break;
        }
        thread::sleep(POLL);
        copied = fs::write(volume.join("glove80.uf2"), &bytes);
    }
    let restarted = (0..40).any(|_| {
        thread::sleep(POLL);
        !volume.exists()
    });
    match copied {
        _ if restarted => {}
        Err(error) => bail!("copying the firmware: {error}"),
        Ok(()) => bail!("the {half} half kept its bootloader volume after the copy"),
    }
    let path = half.flashed().context("no state directory")?;
    write(&path, blake3::hash(&bytes).to_hex().as_str())?;
    println!("Flashed the {half} half.");
    Ok(())
}

fn write(path: &Path, contents: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents).with_context(|| format!("writing {}", path.display()))
}

fn state_dir() -> Option<PathBuf> {
    etcetera::choose_base_strategy()
        .ok()?
        .state_dir()
        .map(|dir| dir.join("accessibility"))
}

fn data_dir() -> Option<PathBuf> {
    etcetera::choose_base_strategy()
        .ok()
        .map(|strategy| strategy.data_dir().join("accessibility"))
}
