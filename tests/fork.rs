// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Check that the lognd fork identifies itself and merges upstream cleanly (see FORK.md).

#![deny(warnings, clippy::all)]

use std::path::Path;
use std::process::Command;

const NOTE_START: &str = "<!-- lognd-fork-note:start -->";
const NOTE_END: &str = "<!-- lognd-fork-note:end -->";

fn read(file: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(file)).unwrap()
}

#[test]
fn fork_page_covers_features_install_upstream_issues_and_tags() {
    let fork = read("FORK.md");
    for heading in [
        "## What the fork adds",
        "## Installing the fork",
        "## Reporting issues",
        "## Keeping up with upstream",
        "### Release tags",
    ] {
        assert!(
            fork.lines().any(|line| line == heading),
            "FORK.md lacks {heading:?}"
        );
    }
    assert!(fork.contains("scripts/sync-upstream.sh"));
    assert!(fork.contains("git push --tags"));
}

#[test]
fn readme_starts_with_the_fork_note_and_is_otherwise_upstream() {
    let readme = read("README.md");
    assert!(
        readme.starts_with(NOTE_START),
        "README.md must open with the fork note"
    );
    let (note, rest) = readme.split_once(NOTE_END).expect("the fork note ends");
    assert!(note.contains("(FORK.md)"));
    // sync-upstream.sh relies on the note being the fork's only change to README.md.
    assert!(
        !rest.contains("lognd"),
        "fork text below the note in README.md"
    );
    assert!(rest.trim_start().starts_with("# mdcat"));
}

#[test]
fn version_carries_the_fork_suffix() {
    let output = Command::new(env!("CARGO_BIN_EXE_mdcat"))
        .arg("--version")
        .output()
        .unwrap();
    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    assert!(output.status.success());
    assert!(
        stdout.starts_with("mdcat ") && stdout.lines().next().unwrap().ends_with("+lognd"),
        "{stdout}"
    );
}

/// Run scripts/test-sync-upstream.sh, which merges fake upstream releases in a throwaway clone.
#[cfg(unix)]
#[test]
fn sync_upstream_resolves_expected_conflicts_and_stops_on_others() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let has_upstream = Command::new("git")
        .args(["rev-parse", "-q", "--verify", "upstream/main"])
        .current_dir(root)
        .output()
        .is_ok_and(|output| output.status.success());
    if !has_upstream {
        eprintln!("no upstream/main ref in this checkout; skipping");
        return;
    }
    let output = Command::new(root.join("scripts/test-sync-upstream.sh"))
        .current_dir(root)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(
        stderr.contains("test-sync-upstream: all passed"),
        "{stderr}"
    );
}
