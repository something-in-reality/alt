//! `alt setup`: a checklist of the machine's one-time steps, each with its current
//! status, run in order once selected.

use std::{fmt, fs, path::Path, process::Command, thread, time::Duration};

use anyhow::{Context, bail};
use inquire::MultiSelect;

use super::status;
use crate::paths;

/// A half of the Glove80, which flashes from its own bootloader volume.
#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::Display)]
#[strum(serialize_all = "lowercase")]
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
    done: Option<bool>,
}

const POLL: Duration = Duration::from_millis(250);

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
}

impl Step {
    const ALL: [Self; 3] = [
        Self::Permission,
        Self::Glove80(Half::Left),
        Self::Glove80(Half::Right),
    ];

    /// Whether the step is done, where the system can tell: the keyboard's firmware is unknown.
    fn done(self) -> Option<bool> {
        match self {
            Self::Permission => Some(status::daemon().is_some_and(status::trusted)),
            Self::Glove80(_) => None,
        }
    }

    fn run(self) -> anyhow::Result<()> {
        match self {
            Self::Permission => {
                Command::new("/usr/bin/open")
                    .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
                    .status()?;
                println!("Turn on Accessibility; waiting for the app to start with it.");
                while !status::daemon().is_some_and(status::trusted) {
                    thread::sleep(POLL);
                }
                Ok(())
            }
            Self::Glove80(half) => flash(half),
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
        let mark = match self.done {
            Some(true) => "✓",
            Some(false) => "✗",
            None => "·",
        };
        write!(f, "{mark} {}", self.step)
    }
}

/// Shows each step's status, preselects those known to be undone, and runs the selection in order.
pub fn setup() -> anyhow::Result<()> {
    let rows: Vec<Row> = Step::ALL
        .into_iter()
        .map(|step| Row {
            step,
            done: step.done(),
        })
        .collect();
    let pending: Vec<usize> = (0..rows.len())
        .filter(|&i| rows[i].done == Some(false))
        .collect();
    let selected = MultiSelect::new("Set up this machine:", rows)
        .with_default(&pending)
        .prompt()?;
    for Row { step, .. } in selected {
        println!("{step}");
        step.run()?;
    }
    Ok(())
}

/// Copies the firmware to `half`'s bootloader volume and waits for the half to restart.
fn flash(half: Half) -> anyhow::Result<()> {
    let firmware = paths::firmware().context(
        "no alt/glove80.uf2 in $XDG_DATA_HOME or $XDG_DATA_DIRS; enable services.alt.glove80",
    )?;
    let bytes = fs::read(&firmware).with_context(|| format!("reading {}", firmware.display()))?;
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
    println!("Flashed the {half} half.");
    Ok(())
}
