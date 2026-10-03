+++
id = "01M40X39W6C9913YT4177C6S5M"
title = "Resolve book order per directory, README first everywhere, natural sort"
type = "story"
category = "in-progress"
priority = "high"
points = 3
parent = "01M40SN4N73X0X52REQ2V7JZCX"
reporter = "lognd"
created = "2026-10-03T12:51:19Z"
updated = "2026-10-03T13:03:19Z"
persona = "a reader of long documentation sets"
capability = "read a nested doc tree in a sensible order without a SUMMARY.md"
outcome_text = "subdirectories keep their own order and ch2 comes before ch10"
idempotency_key = "mdcat-book-order-per-dir"
scope = ["src/book/**", "tests/**", "mdcat.1.adoc", "README.md", "changelog.d/**"]

[[acceptance]]
text = "Given a directory tree without SUMMARY.md, When the book resolves by path, Then every directory's README.md or index.md comes first within that directory"
bound = true

[[acceptance]]
text = "Given files named with numbers, When sorted by path, Then numbers compare numerically (ch2 before ch10) and case is ignored"
bound = true

[[acceptance]]
text = "Given a subdirectory with its own SUMMARY.md, When the parent resolves in auto order, Then that subtree follows its SUMMARY.md at its place in the parent"
bound = false
+++
