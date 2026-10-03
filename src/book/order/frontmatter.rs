// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Read the navigation weight a docs site generator keeps in a document's frontmatter.

use pulldown_cmark_mdcat::strip_frontmatter;

/// Frontmatter keys that order documents in site generators: Jekyll's just-the-docs, Hugo, and
/// Docusaurus; lower comes first.
const WEIGHT_KEYS: [&str; 3] = ["nav_order", "weight", "sidebar_position"];

/// The navigation weight in `markdown`'s YAML (`key: 1`) or TOML (`key = 1`) frontmatter, if it
/// has one: the first top-level weight key with a number for its value.
pub(super) fn weight(markdown: &str) -> Option<f64> {
    let body = strip_frontmatter(markdown);
    let frontmatter = &markdown[..markdown.len() - body.len()];
    frontmatter.lines().find_map(|line| {
        let (key, value) = line.split_once(':').or_else(|| line.split_once('='))?;
        if key != key.trim_start() || !WEIGHT_KEYS.contains(&key.trim_end()) {
            return None;
        }
        value
            .trim()
            .trim_matches(|c| c == '"' || c == '\'')
            .parse::<f64>()
            .ok()
            .filter(|weight| weight.is_finite())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_yaml_and_toml_weights() {
        assert_eq!(weight("---\ntitle: A\nnav_order: 3\n---\n# A\n"), Some(3.0));
        assert_eq!(weight("+++\nweight = 2.5\n+++\n# B\n"), Some(2.5));
        assert_eq!(
            weight("\u{feff}---\nsidebar_position: \"-1\"\n---\n"),
            Some(-1.0)
        );
    }

    #[test]
    fn ignores_weights_outside_frontmatter_nested_or_not_numbers() {
        assert_eq!(weight("# A\n\nnav_order: 3\n"), None);
        assert_eq!(weight("---\nparent:\n  nav_order: 3\n---\n"), None);
        assert_eq!(weight("---\nnav_order: first\n---\n"), None);
        assert_eq!(weight("---\nnav_order: .nan\nweight: 4\n---\n"), Some(4.0));
        assert_eq!(weight("---\nnav_order: 3\n"), None);
    }
}
