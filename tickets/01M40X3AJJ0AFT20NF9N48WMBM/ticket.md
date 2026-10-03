+++
id = "01M40X3AJJ0AFT20NF9N48WMBM"
title = "Choose the book order with --order, a config default and the o key"
type = "story"
category = "in-progress"
priority = "medium"
points = 3
parent = "01M40SN4N73X0X52REQ2V7JZCX"
reporter = "lognd"
created = "2026-10-03T12:51:20Z"
updated = "2026-10-03T13:24:44Z"
persona = "a reader of long documentation sets"
capability = "switch between detected reading orders from the command line, config or inside the pager"
outcome_text = "when detection guesses wrong I can pick the order myself"
idempotency_key = "mdcat-book-order-toggle"
scope = ["src/book/**", "src/args.rs", "src/main.rs", "src/config.rs", "config.toml.example", "tests/**", "mdcat.1.adoc", "README.md", "changelog.d/**"]

[[links]]
kind = "blocked-by"
target = "01M40X3A3JDVYPHQWFCVWWEK5X"

[[links]]
kind = "blocked-by"
target = "01M40X3AB22EBN7RDCC7T282Z9"

[[acceptance]]
text = "Given --order auto, summary, index, frontmatter or path (or defaults.book_order in the config), When a book resolves, Then only that source is used, falling back to path order where it is absent; summary fails if there is no SUMMARY.md"
bound = true

[[acceptance]]
text = "Given the pager, When o is pressed, Then the book switches to the next order that applies, staying on the current document and keeping scroll positions"
bound = true

[[acceptance]]
text = "Given the status line, When it draws, Then it names the order sources in use and how many documents were appended as unlisted"
bound = true

[[acceptance]]
text = "Given unlisted documents, When the table of contents opens, Then they appear under an Unlisted heading that cannot be selected"
bound = false
+++
