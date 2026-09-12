//! Integration tests that exercise the *real* distrobox/container flow.
//!
//! These are `#[ignore]`d by default because they require a working
//! container backend (Distrobox + Podman/Docker on Linux, WSL2 on
//! Windows, Podman Machine/Lima on macOS), pull a real image, and spin
//! up a real container -- too slow/heavy to run on every `cargo test`.
//! Opt in explicitly once the environment has the tools:
//!
//! ```sh
//! cargo test --test distrobox_integration -- --ignored --test-threads=1
//! ```
//!
//! CI runs this in a dedicated job that installs Distrobox + Podman
//! first (see `.github/workflows/ci.yml`'s `integration` job).
//!
//! **Warning**: these tests genuinely run `dbx up`/`dbx rm`,
//! which modify the real `~/.ssh/config` and `~/.ssh/dbx_config` on
//! whatever machine runs them (that side effect is exactly what's being
//! tested). Only run `--ignored` on a disposable CI runner or a machine
//! where you're fine with that, never blindly on your daily-driver box.

use std::path::PathBuf;
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dbx"))
}

/// Probes for a usable Distrobox installation on whichever host
/// transport this platform would use, so the test can skip cleanly
/// instead of failing when the environment doesn't have the tools
/// (e.g. someone ran `--ignored` without reading the module docs above).
fn distrobox_available() -> bool {
    let probe = if cfg!(target_os = "windows") {
        Command::new("wsl.exe")
            .args(["-e", "sh", "-c", "command -v distrobox"])
            .output()
    } else {
        Command::new("sh")
            .arg("-c")
            .arg("command -v distrobox")
            .output()
    };
    matches!(probe, Ok(out) if out.status.success() && !out.stdout.is_empty())
}

fn unique_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dbx-integration-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// RAII guard ensuring any created test box is removed even if assertions fail.
struct BoxGuard {
    box_name: String,
    dir: PathBuf,
}

impl Drop for BoxGuard {
    fn drop(&mut self) {
        let _ = bin()
            .arg("rm")
            .arg("--force")
            .arg(&self.box_name)
            .current_dir(&self.dir)
            .output();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
#[ignore = "requires a real Distrobox + Podman/Docker environment; run with `--ignored`"]
fn up_then_rm_round_trip() {
    if !distrobox_available() {
        eprintln!("skipping up_then_rm_round_trip: distrobox not found on this host");
        return;
    }

    let box_name = format!("dbx-integration-test-{}", std::process::id());
    let dir = unique_dir("roundtrip");
    let _guard = BoxGuard {
        box_name: box_name.clone(),
        dir: dir.clone(),
    };
    std::fs::write(
        dir.join("dbx.ini"),
        format!("[dev-environment]\nname = \"{box_name}\"\nimage = \"ubuntu:24.04\"\n"),
    )
    .expect("write dbx.ini");

    let up = bin()
        .arg("up")
        .current_dir(&dir)
        .output()
        .expect("run dbx up");
    assert!(
        up.status.success(),
        "dbx up failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&up.stdout),
        String::from_utf8_lossy(&up.stderr)
    );

    // `dbx list` should now show the freshly-assembled box.
    let list = bin()
        .arg("list")
        .current_dir(&dir)
        .output()
        .expect("run dbx list");
    assert!(list.status.success());
    assert!(
        String::from_utf8_lossy(&list.stdout).contains(&box_name),
        "dbx list did not mention {box_name}"
    );

    // `dbx up` should have written a managed Host block for this box.
    let ssh_config_path = dirs::home_dir()
        .expect("home dir")
        .join(".ssh")
        .join("dbx_config");
    let ssh_config_before = std::fs::read_to_string(&ssh_config_path).unwrap_or_default();
    assert!(
        ssh_config_before.contains(&format!("Host {box_name}")),
        "no managed Host block for {box_name} in {}",
        ssh_config_path.display()
    );

    let rm = bin()
        .arg("rm")
        .arg("--force")
        .arg(&box_name)
        .current_dir(&dir)
        .output()
        .expect("run dbx rm");
    assert!(
        rm.status.success(),
        "dbx rm failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&rm.stdout),
        String::from_utf8_lossy(&rm.stderr)
    );

    // ...and `dbx rm` should have cleaned that block back up.
    let ssh_config_after = std::fs::read_to_string(&ssh_config_path).unwrap_or_default();
    assert!(
        !ssh_config_after.contains(&format!("Host {box_name}")),
        "managed Host block for {box_name} was not removed from {}",
        ssh_config_path.display()
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "requires a real Distrobox + Podman/Docker environment; run with `--ignored`"]
fn stop_and_cleanup_round_trip() {
    if !distrobox_available() {
        eprintln!("skipping stop_and_cleanup_round_trip: distrobox not found on this host");
        return;
    }

    let box_name = format!("dbx-stop-test-{}", std::process::id());
    let dir = unique_dir("stop");
    let _guard = BoxGuard {
        box_name: box_name.clone(),
        dir: dir.clone(),
    };

    std::fs::write(
        dir.join("dbx.ini"),
        format!("[dev-environment]\nname = \"{box_name}\"\nimage = \"ubuntu:24.04\"\n"),
    )
    .expect("write dbx.ini");

    let up = bin()
        .arg("up")
        .current_dir(&dir)
        .output()
        .expect("run dbx up");
    assert!(
        up.status.success(),
        "dbx up failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&up.stdout),
        String::from_utf8_lossy(&up.stderr)
    );

    let stop = bin()
        .arg("stop")
        .arg(&box_name)
        .current_dir(&dir)
        .output()
        .expect("run dbx stop");
    assert!(
        stop.status.success(),
        "dbx stop failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&stop.stdout),
        String::from_utf8_lossy(&stop.stderr)
    );

    let rm = bin()
        .arg("rm")
        .arg("--force")
        .arg(&box_name)
        .current_dir(&dir)
        .output()
        .expect("run dbx rm");
    assert!(
        rm.status.success(),
        "dbx rm failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&rm.stdout),
        String::from_utf8_lossy(&rm.stderr)
    );
}

#[test]
#[ignore = "requires a real Distrobox + Podman/Docker environment; run with `--ignored`"]
fn custom_config_assembly_round_trip() {
    if !distrobox_available() {
        eprintln!("skipping custom_config_assembly_round_trip: distrobox not found on this host");
        return;
    }

    let box_name = format!("dbx-custom-test-{}", std::process::id());
    let dir = unique_dir("custom");
    let _guard = BoxGuard {
        box_name: box_name.clone(),
        dir: dir.clone(),
    };

    std::fs::write(
        dir.join("dbx.ini"),
        format!(
            "[dev-environment]\nname = \"{box_name}\"\nimage = \"ubuntu:24.04\"\ninit_hooks = \"touch /tmp/dbx_init_test\"\n"
        ),
    )
    .expect("write dbx.ini");

    let up = bin()
        .arg("up")
        .current_dir(&dir)
        .output()
        .expect("run dbx up");
    assert!(
        up.status.success(),
        "dbx up failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&up.stdout),
        String::from_utf8_lossy(&up.stderr)
    );

    let rm = bin()
        .arg("rm")
        .arg("--force")
        .arg(&box_name)
        .current_dir(&dir)
        .output()
        .expect("run dbx rm");
    assert!(
        rm.status.success(),
        "dbx rm failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&rm.stdout),
        String::from_utf8_lossy(&rm.stderr)
    );
}

#[test]
#[ignore = "requires a real Distrobox + Podman/Docker environment; run with `--ignored`"]
fn list_succeeds_when_distrobox_is_installed() {
    if !distrobox_available() {
        eprintln!(
            "skipping list_succeeds_when_distrobox_is_installed: distrobox not found on this host"
        );
        return;
    }

    let dir = unique_dir("list");
    let output = bin()
        .arg("list")
        .current_dir(&dir)
        .output()
        .expect("run dbx list");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let _ = std::fs::remove_dir_all(&dir);
}
