+++
id = "01M40X3AB22EBN7RDCC7T282Z9"
title = "Order book documents by frontmatter weights"
type = "story"
category = "in-progress"
priority = "medium"
points = 2
parent = "01M40SN4N73X0X52REQ2V7JZCX"
reporter = "lognd"
created = "2026-10-03T12:51:20Z"
updated = "2026-10-03T13:17:20Z"
persona = "a reader of long documentation sets"
capability = "have mdcat honour nav_order, weight or sidebar_position from a docs site"
outcome_text = "Jekyll, Hugo and Docusaurus doc sets read in their site order"
idempotency_key = "mdcat-book-frontmatter-weights"
scope = ["src/book/**", "tests/**", "mdcat.1.adoc", "README.md", "changelog.d/**"]

[[links]]
kind = "blocked-by"
target = "01M40X39W6C9913YT4177C6S5M"

[[acceptance]]
text = "Given docs with nav_order, weight or sidebar_position in YAML or TOML frontmatter, When a directory without SUMMARY.md or index resolves in auto order, Then weighted docs come first by weight, then the rest by path"
bound = true

[[acceptance]]
text = "Given a subdirectory whose README or index has a weight, When its parent orders by weight, Then the subdirectory sorts by that weight"
bound = true
+++
