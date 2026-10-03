// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Test book mode of the command line interface.

#![deny(warnings, clippy::all)]

use std::process::{Command, Output};

fn run_mdcat(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mdcat"))
        .args(args)
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> &str {
    std::str::from_utf8(&output.stdout).unwrap()
}

/// The byte offset of each needle in `haystack`, panicking on a missing one.
fn positions(haystack: &str, needles: &[&str]) -> Vec<usize> {
    needles
        .iter()
        .map(|needle| {
            haystack
                .find(needle)
                .unwrap_or_else(|| panic!("{needle:?} missing from output:\n{haystack}"))
        })
        .collect()
}

fn assert_in_order(output: &Output, needles: &[&str]) {
    assert!(output.status.success(), "{output:?}");
    let positions = positions(stdout(output), needles);
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "out of order: {needles:?} at {positions:?}"
    );
}

const SUMMARY_ORDER: [&str; 4] = [
    "first chapter of the sample book",
    "Install the thing",
    "Use the thing",
    "last chapter of the sample book",
];

#[test]
fn directory_argument_prints_the_book_in_summary_order() {
    let output = run_mdcat(&["tests/book/with-summary"]);
    assert_in_order(&output, &SUMMARY_ORDER);
    assert!(!stdout(&output).contains("never read"));
}

#[test]
fn book_flag_with_summary_file_prints_the_book_in_summary_order() {
    for flag in ["--book", "-b"] {
        let output = run_mdcat(&[flag, "tests/book/with-summary/SUMMARY.md"]);
        assert_in_order(&output, &SUMMARY_ORDER);
    }
}

#[test]
fn directory_without_summary_prints_readme_then_by_path() {
    let output = run_mdcat(&["--book", "tests/book/without-summary"]);
    assert_in_order(
        &output,
        &[
            "A book without a summary",
            "Alpha comes after the README",
            "Only the top README comes first",
            "Zeta has no heading",
        ],
    );
}

#[test]
fn summary_file_without_flag_renders_just_that_file() {
    let output = run_mdcat(&["tests/book/with-summary/SUMMARY.md"]);
    assert!(output.status.success());
    assert!(stdout(&output).contains("Not written yet"));
    assert!(!stdout(&output).contains("Install the thing"));
}

#[test]
fn book_flag_refuses_several_arguments() {
    let output = run_mdcat(&["--book", "tests/book/with-summary", "README.md"]);
    assert!(!output.status.success());
    assert!(std::str::from_utf8(&output.stderr)
        .unwrap()
        .contains("--book takes a single directory or summary file"));
}

#[test]
fn book_refuses_watch() {
    let output = run_mdcat(&["--watch", "tests/book/with-summary"]);
    assert!(!output.status.success());
    assert!(std::str::from_utf8(&output.stderr)
        .unwrap()
        .contains("--watch cannot be combined"));
}

/// Run the pager in a pseudo-terminal from util-linux `script`, answering the terminal queries
/// a plain xterm would, send `keys` once it has drawn, and return its exit status and output.
#[cfg(target_os = "linux")]
fn run_pager_in_pty(target: &str, keys: &[u8]) -> Option<(std::process::ExitStatus, String)> {
    use std::io::{Read, Write};
    use std::process::Stdio;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    let command = format!(
        "stty rows 24 cols 80; exec env -u TMUX -u TERM_PROGRAM TERM=xterm-256color {} {target}",
        env!("CARGO_BIN_EXE_mdcat")
    );
    let mut child = Command::new("script")
        .args(["-qfec", &command, "/dev/null"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buffer = [0; 4096];
        while let Ok(read) = stdout.read(&mut buffer) {
            if read == 0 || sender.send(buffer[..read].to_vec()).is_err() {
                break;
            }
        }
    });

    let replies: [(&[u8], &[u8]); 3] = [
        (b"\x1b[c", b"\x1b[?62;22c"),
        (b"\x1b[5n", b"\x1b[0n"),
        (b"\x1b[6n", b"\x1b[24;1R"),
    ];
    let mut output = Vec::new();
    let mut sent_keys = false;
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().unwrap() {
            while let Ok(chunk) = receiver.recv_timeout(Duration::from_millis(200)) {
                output.extend(chunk);
            }
            return Some((status, String::from_utf8_lossy(&output).into_owned()));
        }
        if let Ok(chunk) = receiver.recv_timeout(Duration::from_millis(50)) {
            for (query, reply) in replies {
                if chunk.windows(query.len()).any(|window| window == query) {
                    stdin.write_all(reply).unwrap();
                }
            }
            output.extend(chunk);
        }
        let drawn = String::from_utf8_lossy(&output).contains("quit");
        if drawn && !sent_keys {
            stdin.write_all(keys).unwrap();
            stdin.flush().unwrap();
            sent_keys = true;
        }
    }
    child.kill().unwrap();
    panic!(
        "pager did not exit; output:\n{}",
        String::from_utf8_lossy(&output)
    );
}

#[cfg(target_os = "linux")]
#[test]
fn pager_quits_on_q_and_restores_the_terminal() {
    let Some((status, output)) = run_pager_in_pty("tests/book/long", b"q") else {
        eprintln!("util-linux script is not available; skipping");
        return;
    };
    assert!(status.success(), "{status:?}");
    let entered = output
        .find("\x1b[?1049h")
        .expect("entered the alternate screen");
    let left = output
        .rfind("\x1b[?1049l")
        .expect("left the alternate screen");
    assert!(entered < left);
    assert!(output.contains("1/2"), "{output}");
    assert!(output.contains("README + by path"), "{output}");
}
