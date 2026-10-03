+++
id = "01M40SNJ2GJH6Q0JXDWTJX99KA"
title = "Resolve book reading order and add the --book flag"
type = "story"
category = "in-progress"
priority = "high"
points = 3
parent = "01M40SN4N73X0X52REQ2V7JZCX"
reporter = "lognd"
created = "2026-10-03T11:51:23Z"
updated = "2026-10-03T11:59:56Z"
persona = "a reader of long documentation sets"
capability = "point mdcat at a directory or SUMMARY.md and get every document in reading order"
outcome_text = "I read a doc set in its intended order without listing files by hand"
idempotency_key = "mdcat-book-order"
scope = ["src/book/**", "src/args.rs", "src/main.rs", "src/lib.rs", "tests/**", "mdcat.1.adoc", "README.md", "changelog.d/**", "src/picker.rs", "src/toc.rs"]

[[acceptance]]
text = "Given a SUMMARY.md with nested mdBook link lists, When the book is resolved, Then the documents come out depth-first in list order, skipping draft entries with empty links and duplicates"
bound = true

[[acceptance]]
text = "Given a directory without SUMMARY.md, When the book is resolved, Then README.md comes first and the remaining .md files follow sorted by path, and the order source reads 'README + by path'"
bound = false

[[acceptance]]
text = "Given mdcat docs/ or mdcat --book docs/ (or --book docs/SUMMARY.md) with stdout not a terminal, When it runs, Then every document is rendered to stdout in reading order"
bound = false

[[acceptance]]
text = "Given mdcat docs/SUMMARY.md without --book, When it runs, Then that single file renders exactly as before"
bound = false
+++
