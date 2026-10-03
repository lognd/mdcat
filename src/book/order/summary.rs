// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Read the chapter list of an mdBook `SUMMARY.md`.

use std::collections::HashSet;

use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use pulldown_cmark_mdcat::markdown_options;
use tracing::{event, Level};

use super::{is_local_markdown, normalise};

/// A chapter link found in a summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SummaryEntry {
    /// The link target, relative to the summary, without any `#fragment`.
    pub(super) target: String,
    /// The link text.
    pub(super) title: String,
    /// How many lists the link is nested in, less one; 0 outside lists too.
    pub(super) depth: usize,
}

/// Collect the chapter links of an mdBook summary in depth-first (document) order.
///
/// Skips draft chapters (empty link targets), links to anything but local Markdown files, and
/// repeated targets, keeping the first occurrence.
pub(super) fn parse_summary(markdown: &str) -> Vec<SummaryEntry> {
    let mut entries = Vec::new();
    let mut seen = HashSet::new();
    let mut current: Option<SummaryEntry> = None;
    let mut lists = 0_usize;
    for event in Parser::new_ext(markdown, markdown_options(false)) {
        match event {
            Event::Start(Tag::List(_)) => lists += 1,
            Event::End(TagEnd::List(_)) => lists = lists.saturating_sub(1),
            Event::Start(Tag::Link { dest_url, .. }) => {
                let target = dest_url.split('#').next().unwrap_or_default();
                current = Some(SummaryEntry {
                    target: target.to_string(),
                    title: String::new(),
                    depth: lists.saturating_sub(1),
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

#[cfg(test)]
mod tests {
    use super::*;

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
                ("preface.md", "Preface", 0),
                ("intro.md", "Intro", 0),
                ("guide/setup.md", "Setup", 1),
                ("guide/deep.md", "Deep", 2),
                ("guide/usage.md", "Usage", 1),
                ("appendix.md", "Appendix A", 0),
            ]
            .into_iter()
            .map(|(target, title, depth)| SummaryEntry {
                target: target.to_string(),
                title: title.to_string(),
                depth,
            })
            .collect::<Vec<_>>()
        );
    }
}
