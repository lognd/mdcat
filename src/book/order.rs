// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Resolve the reading order of a book: from an mdBook `SUMMARY.md`, or README first, then by path.

use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use pulldown_cmark_mdcat::markdown_options;
use tracing::{event, Level};

use mdcat::toc::first_heading;

use crate::picker::find_markdown_files;

/// The file name of the mdBook summary that defines a book's reading order.
pub const SUMMARY_FILE: &str = "SUMMARY.md";

/// Where a book's reading order came from, shown to the reader in the status line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderSource {
    /// The links of an mdBook `SUMMARY.md`, depth-first.
    Summary,
    /// No summary: `README.md` first, then every other Markdown file sorted by path.
    ReadmeThenPath,
}

impl fmt::Display for OrderSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrderSource::Summary => f.write_str(SUMMARY_FILE),
            OrderSource::ReadmeThenPath => f.write_str("README + by path"),
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
}

/// A set of Markdown documents in reading order.
#[derive(Debug, Clone)]
pub struct Book {
    /// Where the reading order came from.
    pub order: OrderSource,
    /// The documents in reading order; never empty.
    pub chapters: Vec<Chapter>,
}

impl Book {
    /// Resolve the book at `target`: a directory, or a `SUMMARY.md` file.
    ///
    /// A directory uses its `SUMMARY.md` if it has one, and otherwise every Markdown file below
    /// it (honouring `.gitignore`), `README.md` first. Fails if `target` is neither, if the
    /// summary cannot be read, or if the book has no documents.
    pub fn resolve(target: &Path) -> Result<Book> {
        let book = if target.is_dir() {
            let summary = target.join(SUMMARY_FILE);
            if summary.is_file() {
                Book::from_summary(&summary)?
            } else {
                Book::from_directory(target)
            }
        } else if target.is_file() {
            Book::from_summary(target)?
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
            order = %book.order,
            chapters = book.chapters.len(),
            "Resolved book {}",
            target.display()
        );
        Ok(book)
    }

    /// The book's documents in reading order, as file names for [`mdcat::process_file`].
    pub fn filenames(&self) -> Vec<String> {
        self.chapters
            .iter()
            .map(|chapter| chapter.path.to_string_lossy().into_owned())
            .collect()
    }

    fn from_summary(summary: &Path) -> Result<Book> {
        let markdown = std::fs::read_to_string(summary)
            .with_context(|| format!("Failed to read {}", summary.display()))?;
        let root = summary.parent().unwrap_or_else(|| Path::new(""));
        let chapters = parse_summary(&markdown)
            .into_iter()
            .map(|entry| {
                let path = root.join(&entry.target);
                let title = if entry.title.trim().is_empty() {
                    title_from_file(&path)
                } else {
                    entry.title.trim().to_string()
                };
                Chapter { path, title }
            })
            .collect();
        Ok(Book {
            order: OrderSource::Summary,
            chapters,
        })
    }

    fn from_directory(directory: &Path) -> Book {
        let mut files = find_markdown_files(directory);
        files.sort_by_cached_key(|path| {
            let relative = path.strip_prefix(directory).unwrap_or(path).to_path_buf();
            (!is_readme(&relative), relative)
        });
        let chapters = files
            .into_iter()
            .map(|path| Chapter {
                title: title_from_file(&path),
                path,
            })
            .collect();
        Book {
            order: OrderSource::ReadmeThenPath,
            chapters,
        }
    }
}

/// Whether `relative` is the README at the top of the book.
fn is_readme(relative: &Path) -> bool {
    relative.components().count() == 1
        && relative
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("README.md"))
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

/// A chapter link found in a summary.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SummaryEntry {
    /// The link target, relative to the summary, without any `#fragment`.
    target: String,
    /// The link text.
    title: String,
}

/// Collect the chapter links of an mdBook summary in depth-first (document) order.
///
/// Skips draft chapters (empty link targets), links to anything but local Markdown files, and
/// repeated targets, keeping the first occurrence.
fn parse_summary(markdown: &str) -> Vec<SummaryEntry> {
    let mut entries = Vec::new();
    let mut seen = HashSet::new();
    let mut current: Option<SummaryEntry> = None;
    for event in Parser::new_ext(markdown, markdown_options(false)) {
        match event {
            Event::Start(Tag::Link { dest_url, .. }) => {
                let target = dest_url.split('#').next().unwrap_or_default();
                current = Some(SummaryEntry {
                    target: target.to_string(),
                    title: String::new(),
                });
            }
            Event::Text(text) | Event::Code(text) => {
                if let Some(entry) = current.as_mut() {
                    entry.title.push_str(&text);
                }
            }
            Event::End(TagEnd::Link) => {
                let Some(entry) = current.take() else {
                    continue;
                };
                if !is_local_markdown(&entry.target) {
                    event!(
                        target: "mdcat::book",
                        Level::DEBUG,
                        target = entry.target,
                        "Skipping summary link that is not a local Markdown document"
                    );
                } else if !seen.insert(normalise(&entry.target)) {
                    event!(
                        target: "mdcat::book",
                        Level::DEBUG,
                        target = entry.target,
                        "Skipping repeated summary link"
                    );
                } else {
                    entries.push(entry);
                }
            }
            _ => {}
        }
    }
    entries
}

/// Whether a summary link target names a local Markdown file, not a URL, a fragment or a draft.
fn is_local_markdown(target: &str) -> bool {
    !target.is_empty()
        && url::Url::parse(target).is_err()
        && Path::new(target)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}

/// Normalise a summary link target so that `./a.md` and `a.md` count as the same chapter.
fn normalise(target: &str) -> PathBuf {
    Path::new(target)
        .components()
        .filter(|component| !matches!(component, std::path::Component::CurDir))
        .collect()
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
            .map(|chapter| {
                chapter
                    .path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect()
    }

    #[test]
    fn summary_links_come_out_depth_first() {
        let summary = "# Summary\n\n[Preface](preface.md)\n\n\
            # Part one\n\n\
            - [Intro](intro.md)\n  - [Setup](guide/setup.md#install)\n    - [Deep](guide/deep.md)\n  - [Usage](guide/usage.md)\n\
            - [Draft]()\n\
            ---\n\n\
            - [Again](./intro.md)\n- [Site](https://example.com/x.md)\n- [Image](logo.png)\n\n\
            [Appendix `A`](appendix.md)\n";
        let entries = parse_summary(summary);
        assert_eq!(
            entries,
            vec![
                ("preface.md", "Preface"),
                ("intro.md", "Intro"),
                ("guide/setup.md", "Setup"),
                ("guide/deep.md", "Deep"),
                ("guide/usage.md", "Usage"),
                ("appendix.md", "Appendix A"),
            ]
            .into_iter()
            .map(|(target, title)| SummaryEntry {
                target: target.to_string(),
                title: title.to_string(),
            })
            .collect::<Vec<_>>()
        );
    }

    #[test]
    fn directory_with_summary_uses_summary_order() {
        let root = fixture("with-summary");
        let book = Book::resolve(&root).unwrap();
        assert_eq!(book.order, OrderSource::Summary);
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
        assert_eq!(book.order, OrderSource::Summary);
        assert_eq!(book.chapters.len(), 4);
    }

    #[test]
    fn directory_without_summary_reads_readme_then_by_path() {
        let root = fixture("without-summary");
        let book = Book::resolve(&root).unwrap();
        assert_eq!(book.order, OrderSource::ReadmeThenPath);
        assert_eq!(book.order.to_string(), "README + by path");
        assert_eq!(
            relative_paths(&book, &root),
            vec!["README.md", "alpha.md", "nested/README.md", "zeta.md"]
        );
        // Titled by first heading, or by file stem without one.
        assert_eq!(book.chapters[0].title, "Plain book");
        assert_eq!(book.chapters[3].title, "zeta");
    }

    #[test]
    fn missing_target_is_an_error() {
        let error = Book::resolve(&fixture("does-not-exist")).unwrap_err();
        assert!(error.to_string().contains("neither a directory"));
    }

    #[test]
    fn readme_only_counts_at_the_top() {
        assert!(is_readme(Path::new("readme.MD")));
        assert!(!is_readme(Path::new("nested/README.md")));
    }
}
