// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Resolve the reading order of a book, one directory at a time.
//!
//! Each directory takes its order from the most explicit source it has: an mdBook `SUMMARY.md`;
//! else an index document, a `README.md` or `index.md` whose tables and lists name most of the
//! directory, followed by what it leaves out; else its `README.md` or `index.md` first, then the
//! documents and subdirectories with a frontmatter weight by weight, then the rest in natural name
//! order. Subdirectories resolve the same way in their place.

use std::cmp::Ordering;
use std::collections::HashSet;
use std::fmt;
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};
use mdcat::toc::first_heading;
use pulldown_cmark_mdcat::markdown_options;
use tracing::{event, Level};

use crate::picker::find_markdown_files;

mod frontmatter;
mod index;
mod natural;
mod summary;

use index::index_references;
use natural::natural_cmp;
use summary::parse_summary;

/// The file name of the mdBook summary that defines a book's reading order.
pub const SUMMARY_FILE: &str = "SUMMARY.md";

/// File names that introduce their directory and come first in it, most preferred first.
const INDEX_FILES: [&str; 2] = ["README.md", "index.md"];

/// The fewest documents and subdirectories an index document must name to order its directory,
/// unless the directory has fewer.
const INDEX_MIN_ENTRIES: usize = 3;

/// Where part of a book's reading order came from, shown to the reader in the status line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderSource {
    /// The links of an mdBook summary, depth-first; the path is relative to the book.
    Summary(PathBuf),
    /// The documents an index document lists, in first-mention order; the path is relative to
    /// the book.
    Index(PathBuf),
    /// Navigation weights in the documents' frontmatter, like `nav_order` or `weight`.
    Frontmatter,
    /// No declared order: the directory's README or index first, then by name.
    Path,
}

impl fmt::Display for OrderSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrderSource::Summary(path) | OrderSource::Index(path) => {
                write!(f, "{}", display_path(path))
            }
            OrderSource::Frontmatter => f.write_str("frontmatter"),
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
    /// Whether an index document ordered the document's directory but left the document out,
    /// so that it was appended after the listed ones.
    pub unlisted: bool,
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
    /// Resolve the book at `target`: a directory, or a summary or index file.
    ///
    /// A file named `SUMMARY.md` reads as a summary; any other file as the index of its
    /// directory, however few documents it names. Fails if `target` is neither a directory nor a
    /// file, if a file given as `target` cannot be read, or if the book has no documents.
    pub fn resolve(target: &Path) -> Result<Book> {
        let book = if target.is_dir() {
            let mut resolver = Resolver::new(target);
            resolver.directory(target);
            resolver.finish()
        } else if target.is_file() {
            let directory = target.parent().unwrap_or_else(|| Path::new(""));
            let mut resolver = Resolver::new(directory);
            let is_summary = target
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case(SUMMARY_FILE));
            if is_summary {
                resolver.summary(directory, target)?;
            } else {
                let markdown = std::fs::read_to_string(target)
                    .with_context(|| format!("Failed to read {}", target.display()))?;
                let index = resolver.read_index(directory, target, &markdown);
                resolver.index(directory, index);
            }
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

    /// Describe where the reading order came from, and how many documents an index left out,
    /// for the status line.
    pub fn order_label(&self) -> String {
        let sources = match self.sources.as_slice() {
            [OrderSource::Path] => "README + by path".to_string(),
            sources => sources
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", "),
        };
        match self.unlisted() {
            0 => sources,
            unlisted => format!("{sources} +{unlisted} unlisted"),
        }
    }

    /// How many documents index documents left out.
    pub fn unlisted(&self) -> usize {
        self.chapters
            .iter()
            .filter(|chapter| chapter.unlisted)
            .count()
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
    /// How many index documents are placing what they leave out; above 0 marks documents placed
    /// as unlisted.
    unlisted: usize,
}

/// What an index document names: a document, or a subdirectory to place whole.
#[derive(Debug, Clone, PartialEq, Eq)]
enum IndexEntry {
    Document(PathBuf),
    Directory(PathBuf),
}

/// A candidate index document and the entries it names, resolved and deduplicated.
#[derive(Debug)]
struct Index {
    path: PathBuf,
    entries: Vec<IndexEntry>,
    /// How many of the directory's documents and subdirectories the entries reach.
    covered: usize,
}

impl Resolver {
    fn new(root: &Path) -> Self {
        Resolver {
            root: root.to_path_buf(),
            files: find_markdown_files(root),
            chapters: Vec::new(),
            sources: Vec::new(),
            placed: HashSet::new(),
            unlisted: 0,
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
        match self.find_index(directory) {
            Some(index) => self.index(directory, index),
            None => self.by_path(directory),
        }
    }

    /// The index document of `directory`: the README or index naming the most of its documents
    /// and subdirectories, if it names at least half of them and at least
    /// [`INDEX_MIN_ENTRIES`] (or all, if there are fewer).
    fn find_index(&self, directory: &Path) -> Option<Index> {
        let children = self.children(directory);
        let total = children.len().saturating_sub(1);
        let index = children
            .iter()
            .filter(|(path, is_directory)| !is_directory && index_rank(&file_name(path)) < INDEX_FILES.len())
            .filter_map(|(path, _)| {
                let markdown = std::fs::read_to_string(path)
                    .inspect_err(|error| {
                        event!(target: "mdcat::book", Level::WARN, %error, "Cannot read {}", path.display());
                    })
                    .ok()?;
                Some(self.read_index(directory, path, &markdown))
            })
            .max_by(|left, right| {
                left.covered.cmp(&right.covered).then_with(|| {
                    // Prefer README.md over index.md on a tie.
                    index_rank(&file_name(&right.path)).cmp(&index_rank(&file_name(&left.path)))
                })
            })?;
        let enough = index.covered > 0
            && index.covered >= INDEX_MIN_ENTRIES.min(total)
            && index.covered * 2 >= total;
        event!(
            target: "mdcat::book",
            Level::DEBUG,
            covered = index.covered,
            total,
            enough,
            "Candidate index {}",
            index.path.display()
        );
        enough.then_some(index)
    }

    /// Resolve the references of the index document at `path` (in `directory`) to the documents
    /// and subdirectories below `directory` they name, in first-mention order, once each.
    fn read_index(&self, directory: &Path, path: &Path, markdown: &str) -> Index {
        let base = path.parent().unwrap_or(directory);
        let directory = normalise(directory);
        let own = normalise(path);
        let mut entries: Vec<IndexEntry> = Vec::new();
        let mut covered = HashSet::new();
        for reference in index_references(pulldown_cmark_mdcat::strip_frontmatter(markdown)) {
            let Some(entry) = self.resolve_reference(&directory, base, &reference) else {
                event!(
                    target: "mdcat::book",
                    Level::DEBUG,
                    reference,
                    "Skipping index reference to nothing in {}",
                    directory.display()
                );
                continue;
            };
            let target = match &entry {
                IndexEntry::Document(path) | IndexEntry::Directory(path) => path,
            };
            if *target == own || entries.contains(&entry) {
                continue;
            }
            if let Some(child) = target
                .strip_prefix(&directory)
                .ok()
                .and_then(|relative| relative.components().next())
            {
                covered.insert(directory.join(child));
            }
            entries.push(entry);
        }
        Index {
            path: own,
            entries,
            covered: covered.len(),
        }
    }

    /// The document or subdirectory below `directory` that `reference`, relative to `base`,
    /// names, if it exists: a subdirectory's README or index names the subdirectory.
    fn resolve_reference(
        &self,
        directory: &Path,
        base: &Path,
        reference: &str,
    ) -> Option<IndexEntry> {
        if reference.is_empty() || url::Url::parse(reference).is_ok() || reference.starts_with('/')
        {
            return None;
        }
        let target = normalise(base.join(reference));
        if target == directory || !target.starts_with(directory) {
            return None;
        }
        let is_file = self.files.iter().any(|file| normalise(file) == target);
        if is_file {
            let parent = target.parent().unwrap_or(directory);
            let introduces_subdirectory =
                parent != directory && index_rank(&file_name(&target)) < INDEX_FILES.len();
            return Some(if introduces_subdirectory {
                IndexEntry::Directory(parent.to_path_buf())
            } else {
                IndexEntry::Document(target)
            });
        }
        let holds_documents = self
            .files
            .iter()
            .any(|file| normalise(file).starts_with(&target));
        holds_documents.then_some(IndexEntry::Directory(target))
    }

    /// Place `index` and the entries it names, then the rest of `directory` as unlisted.
    fn index(&mut self, directory: &Path, index: Index) {
        event!(
            target: "mdcat::book",
            Level::DEBUG,
            entries = index.entries.len(),
            "Ordering {} by {}",
            directory.display(),
            index.path.display()
        );
        self.note(OrderSource::Index(self.relative(&index.path)));
        let level = self.level(directory);
        self.place(index.path, None, level);
        for entry in index.entries {
            match entry {
                IndexEntry::Document(path) => {
                    let level = self.level(path.parent().unwrap_or(directory));
                    self.place(path, None, level);
                }
                IndexEntry::Directory(path) => self.directory(&path),
            }
        }
        self.unlisted += 1;
        self.by_path(directory);
        self.unlisted -= 1;
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

    /// Place `directory`'s README or index first, then its documents and subdirectories with a
    /// frontmatter weight by weight, then the rest by name.
    fn by_path(&mut self, directory: &Path) {
        let mut children: Vec<_> = self
            .children(directory)
            .into_iter()
            .map(|(child, is_directory)| {
                let weight = self.weight(&child, is_directory);
                let name = file_name(&child);
                let rank = if is_directory {
                    INDEX_FILES.len()
                } else {
                    index_rank(&name)
                };
                (child, is_directory, rank, weight, name)
            })
            .collect();
        if self.unlisted == 0
            && children
                .iter()
                .any(|(_, _, rank, weight, _)| *rank == INDEX_FILES.len() && weight.is_some())
        {
            self.note(OrderSource::Frontmatter);
        }
        children.sort_by(|left, right| {
            let (_, _, left_rank, left_weight, left_name) = left;
            let (_, _, right_rank, right_weight, right_name) = right;
            left_rank
                .cmp(right_rank)
                .then_with(|| match (left_weight, right_weight) {
                    (Some(left), Some(right)) => left.total_cmp(right),
                    (Some(_), None) => Ordering::Less,
                    (None, Some(_)) => Ordering::Greater,
                    (None, None) => Ordering::Equal,
                })
                .then_with(|| natural_cmp(left_name, right_name))
        });
        let children: Vec<_> = children
            .into_iter()
            .map(|(child, is_directory, ..)| (child, is_directory))
            .collect();
        let level = self.level(directory);
        for (child, is_directory) in children {
            if is_directory {
                self.directory(&child);
            } else {
                if self.unlisted == 0 {
                    self.note(OrderSource::Path);
                }
                self.place(child, None, level);
            }
        }
    }

    /// The frontmatter weight of a document, or of a subdirectory's README or index.
    fn weight(&self, child: &Path, is_directory: bool) -> Option<f64> {
        let document = if is_directory {
            self.files
                .iter()
                .filter(|file| file.parent() == Some(child))
                .min_by_key(|file| index_rank(&file_name(file)))
                .filter(|file| index_rank(&file_name(file)) < INDEX_FILES.len())?
                .clone()
        } else {
            child.to_path_buf()
        };
        let markdown = std::fs::read_to_string(&document).ok()?;
        frontmatter::weight(&markdown)
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
        event!(
            target: "mdcat::book",
            Level::TRACE,
            depth,
            unlisted = self.unlisted > 0,
            "Placed {}",
            path.display()
        );
        self.chapters.push(Chapter {
            path,
            title,
            depth,
            unlisted: self.unlisted > 0,
        });
    }

    /// Record that part of the order came from `source`.
    fn note(&mut self, source: OrderSource) {
        if !self.sources.contains(&source) {
            self.sources.push(source);
        }
    }

    /// How many directories `directory` is below the root.
    fn level(&self, directory: &Path) -> usize {
        normalise(directory)
            .strip_prefix(normalise(&self.root))
            .map_or(0, |relative| relative.components().count())
    }

    /// `path` relative to the root, for display.
    fn relative(&self, path: &Path) -> PathBuf {
        let path = normalise(path);
        path.strip_prefix(normalise(&self.root))
            .map(Path::to_path_buf)
            .unwrap_or(path)
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

/// The file name of `path` as a string, empty for none.
fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
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

    fn unlisted(book: &Book) -> Vec<bool> {
        book.chapters
            .iter()
            .map(|chapter| chapter.unlisted)
            .collect()
    }

    #[test]
    fn index_table_orders_its_directory_and_appends_the_rest() {
        let root = fixture("index-table");
        let book = Book::resolve(&root).unwrap();
        assert_eq!(
            relative_paths(&book, &root),
            vec![
                "README.md",
                "goals.md",
                "cli.md",
                "architecture.md",
                "sub/README.md",
                "sub/a.md",
                "sub/b.md",
                "alpha.md",
                "other/o.md",
                "zeta.md",
            ]
        );
        assert_eq!(
            unlisted(&book),
            vec![false, false, false, false, false, false, false, true, true, true]
        );
        assert_eq!(depths(&book), vec![0, 0, 0, 0, 1, 1, 1, 0, 1, 0]);
        assert_eq!(book.unlisted(), 3);
        assert_eq!(book.order_label(), "README.md, by path +3 unlisted");
    }

    #[test]
    fn index_file_target_is_read_as_the_index_of_its_directory() {
        let root = fixture("weak-index");
        let book = Book::resolve(&root.join("README.md")).unwrap();
        assert_eq!(
            relative_paths(&book, &root),
            vec!["README.md", "a.md", "b.md", "c.md", "d.md", "e.md", "f.md"]
        );
        assert_eq!(book.unlisted(), 4);
        assert_eq!(book.sources, vec![OrderSource::Index("README.md".into())]);
    }

    #[test]
    fn a_readme_naming_too_little_is_not_an_index() {
        let root = fixture("weak-index");
        let book = Book::resolve(&root).unwrap();
        assert_eq!(book.sources, vec![OrderSource::Path]);
        assert_eq!(book.unlisted(), 0);
    }

    #[test]
    fn index_references_resolve_inside_the_directory_only() {
        let root = fixture("index-table");
        let resolver = Resolver::new(&root);
        let directory = normalise(&root);
        let resolve = |reference: &str| resolver.resolve_reference(&directory, &root, reference);
        assert_eq!(
            resolve("sub/README.md"),
            Some(IndexEntry::Directory(directory.join("sub")))
        );
        assert_eq!(
            resolve("./sub/a.md"),
            Some(IndexEntry::Document(directory.join("sub/a.md")))
        );
        for nothing in [
            "../outside.md",
            "missing.md",
            "https://example.com/a.md",
            "/goals.md",
            ".",
            "",
        ] {
            assert_eq!(resolve(nothing), None, "{nothing}");
        }
    }

    #[test]
    fn frontmatter_weights_order_after_the_readme_and_before_the_rest() {
        let root = fixture("weighted");
        let book = Book::resolve(&root).unwrap();
        assert_eq!(
            relative_paths(&book, &root),
            vec![
                "README.md",
                "guide/README.md",
                "guide/x.md",
                "b.md",
                "a.md",
                "c.md",
                "d.md",
            ]
        );
        assert_eq!(book.order_label(), "frontmatter, by path");
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
