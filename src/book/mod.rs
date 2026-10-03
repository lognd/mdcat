// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Book mode: read a set of Markdown documents in reading order.

use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

mod order;
mod pager;

pub use order::Book;
pub use pager::{max_columns, run as page, PagerImages, PagerOptions};

/// The directory or summary to read as a book, or `None` outside book mode.
///
/// `book` is the `--book` flag; without it a single directory argument still selects book mode,
/// since mdcat cannot render a directory as one document. With `--book`, standard input (`-`,
/// the default) means the current directory. Fails if `--book` gets more than one argument.
pub fn book_target(filenames: &[String], book: bool) -> Result<Option<PathBuf>> {
    match filenames {
        [single] if book && single == "-" => Ok(Some(PathBuf::from("."))),
        [single] if book || Path::new(single).is_dir() => Ok(Some(PathBuf::from(single))),
        _ if book => bail!("--book takes a single directory or summary file"),
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(filenames: &[&str], book: bool) -> Result<Option<PathBuf>> {
        let filenames: Vec<String> = filenames.iter().map(ToString::to_string).collect();
        book_target(&filenames, book)
    }

    #[test]
    fn single_file_without_flag_is_not_a_book() {
        assert_eq!(
            target(&["tests/book/with-summary/SUMMARY.md"], false).unwrap(),
            None
        );
        assert_eq!(target(&["-"], false).unwrap(), None);
    }

    #[test]
    fn single_directory_implies_book_mode() {
        assert_eq!(
            target(&["tests/book"], false).unwrap(),
            Some(PathBuf::from("tests/book"))
        );
    }

    #[test]
    fn flag_takes_a_file_or_defaults_to_current_directory() {
        assert_eq!(
            target(&["tests/book/with-summary/SUMMARY.md"], true).unwrap(),
            Some(PathBuf::from("tests/book/with-summary/SUMMARY.md"))
        );
        assert_eq!(target(&["-"], true).unwrap(), Some(PathBuf::from(".")));
    }

    #[test]
    fn flag_refuses_several_arguments() {
        assert!(target(&["a.md", "b.md"], true).is_err());
        assert_eq!(target(&["tests/book", "a.md"], false).unwrap(), None);
    }
}
