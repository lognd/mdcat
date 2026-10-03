// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Read the documents an index document (a directory's `README.md` or `index.md`) lists.
//!
//! An index lists documents in tables and lists, by link or just by name. Names in prose do not
//! count, since prose names files for every reason but reading order ("where these files
//! disagree, products.md wins").

use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use pulldown_cmark_mdcat::markdown_options;

/// The file extension that marks a bare name in a table or list as a document.
const MARKDOWN_EXTENSION: &str = ".md";

/// Collect the documents `markdown` references in first-mention order, without `#fragment`s.
///
/// A reference is a link target anywhere, or a name ending in `.md` in the text or inline code
/// of a table cell or list item. Targets may repeat and may name anything; resolving them
/// against the file system is up to the caller.
pub(super) fn index_references(markdown: &str) -> Vec<String> {
    let mut references = Vec::new();
    // How many table cells and list items enclose the current event.
    let mut structure = 0_usize;
    let mut in_link = false;
    for event in Parser::new_ext(markdown, markdown_options(false)) {
        match event {
            Event::Start(Tag::TableCell | Tag::Item) => structure += 1,
            Event::End(TagEnd::TableCell | TagEnd::Item) => {
                structure = structure.saturating_sub(1);
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                in_link = true;
                references.push(without_fragment(&dest_url).to_string());
            }
            Event::End(TagEnd::Link) => in_link = false,
            Event::Text(text) | Event::Code(text) if structure > 0 && !in_link => {
                references.extend(bare_names(&text).map(ToString::to_string));
            }
            _ => {}
        }
    }
    references
}

fn without_fragment(target: &str) -> &str {
    target.split('#').next().unwrap_or_default()
}

/// The words of `text` that name Markdown files, like `goals.md` or `guide/setup.md`.
fn bare_names(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | '#')))
        .map(|word| without_fragment(word).trim_end_matches('.'))
        .filter(|word| {
            word.len() > MARKDOWN_EXTENSION.len()
                && word
                    .get(word.len() - MARKDOWN_EXTENSION.len()..)
                    .is_some_and(|extension| extension.eq_ignore_ascii_case(MARKDOWN_EXTENSION))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_anywhere_and_names_in_tables_and_lists_count_in_order() {
        let markdown = "# Design\n\n\
            Start with zeta.md, then [alpha](alpha.md).\n\n\
            | File | Covers |\n|---|---|\n\
            | goals.md | the goals, unlike other.md |\n\
            | `cli.md` | the CLI |\n\
            | [Arch](architecture.md#top) | see `ignored.txt` |\n\n\
            - [Sub](sub/) and notes/x.md.\n\
            - goals.md again, and README.MD\n";
        assert_eq!(
            index_references(markdown),
            vec![
                "alpha.md",
                "goals.md",
                "other.md",
                "cli.md",
                "architecture.md",
                "sub/",
                "notes/x.md",
                "goals.md",
                "README.MD",
            ]
        );
    }

    #[test]
    fn link_text_is_not_scanned_for_names() {
        assert_eq!(
            index_references("- [see b.md](a.md)\n"),
            vec!["a.md".to_string()]
        );
    }

    #[test]
    fn bare_names_need_a_name_before_the_extension() {
        assert_eq!(
            bare_names("x .md md a.md. (b.md) c.md#part d.markdown").collect::<Vec<_>>(),
            vec!["a.md", "b.md", "c.md"]
        );
    }
}
