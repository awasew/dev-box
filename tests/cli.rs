//! Fast, hermetic black-box CLI tests.
//!
//! These spawn the real `dev-box` binary (via Cargo's `CARGO_BIN_EXE_*`
//! env var) and only exercise code paths that never touch an external
//! container runtime -- `config`, `up --dry-run`, and argument
//! validation -- so they run everywhere `cargo test` runs, with no
//! Distrobox/Podman/Docker/WSL2 required.
//!
//! For tests that exercise the *real* distrobox/container flow, see
//! `tests/distrobox_integration.rs`.

use std::path::PathBuf;
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dev-box"))
}

/// Creates a fresh, uniquely-named temp directory for a test to use as
/// its working directory, so parallel tests never collide.
fn unique_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "dev-box-clitest-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn help_lists_all_subcommands() {
    let output = bin().arg("--help").output().expect("run dev-box --help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    for cmd in ["up", "enter", "sync", "list", "stop", "rm", "config"] {
        assert!(
            stdout.contains(cmd),
            "--help output missing `{cmd}`:\n{stdout}"
        );
    }
}

#[test]
fn config_merges_layers_and_prints_ini() {
    let dir = unique_dir("config");
    std::fs::write(
        dir.join("devbox.ini"),
        "[dev-environment]\nname = \"cli-test-box\"\nimage = \"ubuntu:24.04\"\n",
    )
    .expect("write devbox.ini");

    let output = bin()
        .arg("config")
        .current_dir(&dir)
        .output()
        .expect("run dev-box config");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("cli-test-box"));
    assert!(stdout.contains("ubuntu:24.04"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn config_merges_project_and_local_layers_with_local_winning() {
    let dir = unique_dir("config-layers");
    std::fs::write(
        dir.join("devbox.ini"),
        "[dev-environment]\nname = \"project-box\"\nimage = \"ubuntu:24.04\"\nadditional_packages = \"git\"\n",
    )
    .expect("write devbox.ini");
    std::fs::write(
        dir.join("devbox.local.ini"),
        "[dev-environment]\nimage = \"ubuntu:22.04\"\nadditional_packages = \"curl\"\n",
    )
    .expect("write devbox.local.ini");

    let output = bin()
        .arg("config")
        .current_dir(&dir)
        .output()
        .expect("run dev-box config");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Scalar key: the local layer overrides the project layer.
    assert!(stdout.contains("ubuntu:22.04"));
    assert!(!stdout.contains("ubuntu:24.04"));
    // Additive key: both layers' values are space-joined.
    assert!(stdout.contains("git") && stdout.contains("curl"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn up_dry_run_prints_merged_config_without_touching_containers() {
    let dir = unique_dir("dryrun");
    std::fs::write(
        dir.join("devbox.ini"),
        "[dev-environment]\nname = \"dry-run-box\"\nimage = \"ubuntu:24.04\"\n",
    )
    .expect("write devbox.ini");

    let output = bin()
        .arg("up")
        .arg("--dry-run")
        .current_dir(&dir)
        .output()
        .expect("run dev-box up --dry-run");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("dry-run-box"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn up_without_a_box_name_fails_with_a_clear_error() {
    let dir = unique_dir("noname");
    // No devbox.ini at all -> no [dev-environment] name= anywhere, and
    // --dry-run is not passed, so `ctx.box_name()` must fail fast
    // before anything tries to touch a container backend.
    let output = bin()
        .arg("up")
        .current_dir(&dir)
        .output()
        .expect("run dev-box up");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
    assert!(stderr.contains("name"), "unexpected stderr: {stderr}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rm_without_a_box_name_or_config_fails_with_a_clear_error() {
    let dir = unique_dir("rm-noname");
    let output = bin()
        .arg("rm")
        .arg("--force")
        .current_dir(&dir)
        .output()
        .expect("run dev-box rm");
    assert!(!output.status.success());

    let _ = std::fs::remove_dir_all(&dir);
}
