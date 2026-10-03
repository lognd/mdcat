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
