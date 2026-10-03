// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Resolve the reading order of a book, one directory at a time.
//!
//! Each directory takes its order from the most explicit source it has: an mdBook `SUMMARY.md`,
//! else its `README.md` or `index.md` first and every other document and subdirectory in natural
//! name order, subdirectories resolved the same way in their place.

use std::collections::HashSet;
use std::fmt;
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};
use mdcat::toc::first_heading;
use pulldown_cmark_mdcat::markdown_options;
use tracing::{event, Level};

use crate::picker::find_markdown_files;

mod natural;
mod summary;

use natural::natural_cmp;
use summary::parse_summary;

/// The file name of the mdBook summary that defines a book's reading order.
pub const SUMMARY_FILE: &str = "SUMMARY.md";

/// File names that introduce their directory and come first in it, most preferred first.
const INDEX_FILES: [&str; 2] = ["README.md", "index.md"];

/// Where part of a book's reading order came from, shown to the reader in the status line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderSource {
    /// The links of an mdBook summary, depth-first; the path is relative to the book.
    Summary(PathBuf),
    /// No declared order: the directory's README or index first, then by name.
    Path,
}

impl fmt::Display for OrderSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrderSource::Summary(path) => write!(f, "{}", display_path(path)),
            OrderSource::Path => f.write_str("by path"),
        }
    }
}

/// One document of a book.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chapter {
    /// The path of the document, relative to the working directory like the book target.
    pub path: PathBuf,
    /// The title to show for the document: its summary link text, else its first heading,
    /// else its file stem.
    pub title: String,
    /// How deeply the document nests: its directory level below the book, plus its list level
    /// in a summary; 0 at the top.
    pub depth: usize,
}

/// A set of Markdown documents in reading order.
#[derive(Debug, Clone)]
pub struct Book {
    /// The documents in reading order; never empty.
    pub chapters: Vec<Chapter>,
    /// Every source the order came from, in the order they were first used.
    pub sources: Vec<OrderSource>,
}

impl Book {
    /// Resolve the book at `target`: a directory, or a summary file.
    ///
    /// Fails if `target` is neither, if a summary given as `target` cannot be read, or if the
    /// book has no documents.
    pub fn resolve(target: &Path) -> Result<Book> {
        let book = if target.is_dir() {
            let mut resolver = Resolver::new(target);
            resolver.directory(target);
            resolver.finish()
        } else if target.is_file() {
            let directory = target.parent().unwrap_or_else(|| Path::new(""));
            let mut resolver = Resolver::new(directory);
            resolver.summary(directory, target)?;
            resolver.finish()
        } else {
            bail!(
                "{} is neither a directory nor a summary file",
                target.display()
            );
        };
        if book.chapters.is_empty() {
            bail!("No Markdown documents to read in {}", target.display());
        }
        event!(
            target: "mdcat::book",
            Level::DEBUG,
            order = book.order_label(),
            chapters = book.chapters.len(),
            "Resolved book {}",
            target.display()
        );
        Ok(book)
    }

    /// Describe where the reading order came from, for the status line.
    pub fn order_label(&self) -> String {
        match self.sources.as_slice() {
            [OrderSource::Path] => "README + by path".to_string(),
            sources => sources
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", "),
        }
    }

    /// The book's documents in reading order, as file names for [`mdcat::process_file`].
    pub fn filenames(&self) -> Vec<String> {
        self.chapters
            .iter()
            .map(|chapter| chapter.path.to_string_lossy().into_owned())
            .collect()
    }
}

/// A book under construction: every Markdown file below its root and what is placed so far.
struct Resolver {
    root: PathBuf,
    /// Every Markdown file below the root, honouring ignore files.
    files: Vec<PathBuf>,
    chapters: Vec<Chapter>,
    sources: Vec<OrderSource>,
    placed: HashSet<PathBuf>,
}

impl Resolver {
    fn new(root: &Path) -> Self {
        Resolver {
            root: root.to_path_buf(),
            files: find_markdown_files(root),
            chapters: Vec::new(),
            sources: Vec::new(),
            placed: HashSet::new(),
        }
    }

    fn finish(self) -> Book {
        Book {
            chapters: self.chapters,
            sources: self.sources,
        }
    }

    /// Place every document of `directory` in its order, recursing into subdirectories.
    fn directory(&mut self, directory: &Path) {
        let summary = directory.join(SUMMARY_FILE);
        if summary.is_file() {
            match self.summary(directory, &summary) {
                Ok(()) => return,
                Err(error) => event!(
                    target: "mdcat::book",
                    Level::WARN,
                    "Ignoring unreadable summary: {error:#}"
                ),
            }
        }
        self.by_path(directory);
    }

    /// Place the chapters `summary` lists, resolving its links against `directory`.
    fn summary(&mut self, directory: &Path, summary: &Path) -> Result<()> {
        let markdown = std::fs::read_to_string(summary)
            .with_context(|| format!("Failed to read {}", summary.display()))?;
        event!(
            target: "mdcat::book",
            Level::DEBUG,
            "Ordering {} by {}",
            directory.display(),
            summary.display()
        );
        self.note(OrderSource::Summary(self.relative(summary)));
        let base = self.level(directory);
        for entry in parse_summary(&markdown) {
            let title = Some(entry.title.trim().to_string()).filter(|title| !title.is_empty());
            self.place(directory.join(&entry.target), title, base + entry.depth);
        }
        Ok(())
    }

    /// Place `directory`'s README or index first, then its documents and subdirectories by name.
    fn by_path(&mut self, directory: &Path) {
        let mut children = self.children(directory);
        children.sort_by(|(left, _), (right, _)| {
            let name = |path: &Path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default()
            };
            let (left, right) = (name(left), name(right));
            index_rank(&left)
                .cmp(&index_rank(&right))
                .then_with(|| natural_cmp(&left, &right))
        });
        let level = self.level(directory);
        for (child, is_directory) in children {
            if is_directory {
                self.directory(&child);
            } else {
                self.note(OrderSource::Path);
                self.place(child, None, level);
            }
        }
    }

    /// The documents and the subdirectories holding documents directly inside `directory`;
    /// the flag is true for subdirectories.
    fn children(&self, directory: &Path) -> Vec<(PathBuf, bool)> {
        let mut seen = HashSet::new();
        self.files
            .iter()
            .filter_map(|file| {
                let mut components = file.strip_prefix(directory).ok()?.components();
                let first = components.next()?;
                let child = directory.join(first);
                seen.insert(child.clone())
                    .then_some((child, components.next().is_some()))
            })
            .collect()
    }

    /// Append the document at `path` unless it is placed already.
    fn place(&mut self, path: PathBuf, title: Option<String>, depth: usize) {
        if !self.placed.insert(normalise(&path)) {
            event!(
                target: "mdcat::book",
                Level::DEBUG,
                "Skipping {}, placed already",
                path.display()
            );
            return;
        }
        let title = title.unwrap_or_else(|| title_from_file(&path));
        self.chapters.push(Chapter { path, title, depth });
    }

    /// Record that part of the order came from `source`.
    fn note(&mut self, source: OrderSource) {
        if !self.sources.contains(&source) {
            self.sources.push(source);
        }
    }

    /// How many directories `directory` is below the root.
    fn level(&self, directory: &Path) -> usize {
        directory
            .strip_prefix(&self.root)
            .map_or(0, |relative| relative.components().count())
    }

    /// `path` relative to the root, for display.
    fn relative(&self, path: &Path) -> PathBuf {
        path.strip_prefix(&self.root).unwrap_or(path).to_path_buf()
    }
}

/// Where a file name sorts among the files introducing a directory, after every one if it is
/// none of them.
fn index_rank(name: &str) -> usize {
    INDEX_FILES
        .iter()
        .position(|index| index.eq_ignore_ascii_case(name))
        .unwrap_or(INDEX_FILES.len())
}

/// Title a document by its first heading, falling back to its file stem.
fn title_from_file(path: &Path) -> String {
    let heading = std::fs::read_to_string(path).ok().and_then(|markdown| {
        first_heading(
            pulldown_cmark_mdcat::strip_frontmatter(&markdown),
            markdown_options(false),
        )
    });
    heading.unwrap_or_else(|| {
        event!(
            target: "mdcat::book",
            Level::TRACE,
            "No heading in {}, titling it by file name",
            path.display()
        );
        path.file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string())
    })
}

/// Whether a link target names a local Markdown file, not a URL, a fragment or a draft.
fn is_local_markdown(target: &str) -> bool {
    !target.is_empty()
        && url::Url::parse(target).is_err()
        && Path::new(target)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}

/// Normalise a path so that `./a.md`, `a.md` and `b/../a.md` count as the same document.
fn normalise(path: impl AsRef<Path>) -> PathBuf {
    let mut normal = PathBuf::new();
    for component in path.as_ref().components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir if normal.file_name().is_some() => {
                normal.pop();
            }
            component => normal.push(component),
        }
    }
    normal
}

/// Show a relative path with forward slashes on every platform.
fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("book")
            .join(name)
    }

    fn relative_paths(book: &Book, root: &Path) -> Vec<String> {
        book.chapters
            .iter()
            .map(|chapter| display_path(chapter.path.strip_prefix(root).unwrap()))
            .collect()
    }

    fn depths(book: &Book) -> Vec<usize> {
        book.chapters.iter().map(|chapter| chapter.depth).collect()
    }

    #[test]
    fn directory_with_summary_uses_summary_order() {
        let root = fixture("with-summary");
        let book = Book::resolve(&root).unwrap();
        assert_eq!(book.order_label(), "SUMMARY.md");
        assert_eq!(
            relative_paths(&book, &root),
            vec![
                "intro.md",
                "guide/setup.md",
                "guide/usage.md",
                "appendix.md"
            ]
        );
        assert_eq!(book.chapters[0].title, "Introduction");
        assert_eq!(
            book.filenames(),
            book.chapters
                .iter()
                .map(|chapter| chapter.path.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn summary_file_target_uses_summary_order() {
        let root = fixture("with-summary");
        let book = Book::resolve(&root.join(SUMMARY_FILE)).unwrap();
        assert_eq!(
            book.sources,
            vec![OrderSource::Summary(SUMMARY_FILE.into())]
        );
        assert_eq!(book.chapters.len(), 4);
    }

    #[test]
    fn directory_without_summary_reads_readme_then_by_path() {
        let root = fixture("without-summary");
        let book = Book::resolve(&root).unwrap();
        assert_eq!(book.sources, vec![OrderSource::Path]);
        assert_eq!(book.order_label(), "README + by path");
        assert_eq!(
            relative_paths(&book, &root),
            vec!["README.md", "alpha.md", "nested/README.md", "zeta.md"]
        );
        assert_eq!(depths(&book), vec![0, 0, 1, 0]);
        // Titled by first heading, or by file stem without one.
        assert_eq!(book.chapters[0].title, "Plain book");
        assert_eq!(book.chapters[3].title, "zeta");
    }

    #[test]
    fn every_directory_puts_its_index_first_and_sorts_naturally() {
        let root = fixture("nested");
        let book = Book::resolve(&root).unwrap();
        assert_eq!(
            relative_paths(&book, &root),
            vec![
                "README.md",
                "Appendix.md",
                "ch2.md",
                "ch10.md",
                "part/index.md",
                "part/x.md",
                "summarised/two.md",
                "summarised/one.md",
            ]
        );
        assert_eq!(depths(&book), vec![0, 0, 0, 0, 1, 1, 1, 2]);
        assert_eq!(
            book.sources,
            vec![
                OrderSource::Path,
                OrderSource::Summary("summarised/SUMMARY.md".into())
            ]
        );
        assert_eq!(book.order_label(), "by path, summarised/SUMMARY.md");
    }

    #[test]
    fn missing_target_is_an_error() {
        let error = Book::resolve(&fixture("does-not-exist")).unwrap_err();
        assert!(error.to_string().contains("neither a directory"));
    }

    #[test]
    fn readme_comes_before_index() {
        assert_eq!(index_rank("readme.MD"), 0);
        assert_eq!(index_rank("Index.md"), 1);
        assert_eq!(index_rank("intro.md"), 2);
    }

    #[test]
    fn normalise_folds_current_and_parent_directories() {
        assert_eq!(normalise("./a/../b/./c.md"), PathBuf::from("b/c.md"));
        assert_eq!(normalise("../c.md"), PathBuf::from("../c.md"));
    }
}
