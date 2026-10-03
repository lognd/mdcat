+++
id = "01M40X3A3JDVYPHQWFCVWWEK5X"
title = "Detect reading order from index documents (README/index tables and lists)"
type = "story"
category = "done"
outcome = "done"
priority = "high"
points = 5
parent = "01M40SN4N73X0X52REQ2V7JZCX"
reporter = "lognd"
created = "2026-10-03T12:51:19Z"
updated = "2026-10-03T13:12:07Z"
persona = "a reader of long documentation sets"
capability = "have mdcat follow the reading order a README table or list already gives"
outcome_text = "doc sets without a SUMMARY.md, like frob-v2's design docs, read in their intended order"
idempotency_key = "mdcat-book-index-detection"
scope = ["src/book/**", "tests/**", "mdcat.1.adoc", "README.md", "changelog.d/**"]

[[links]]
kind = "blocked-by"
target = "01M40X39W6C9913YT4177C6S5M"

[[acceptance]]
text = "Given a README.md whose table cells or list items name sibling docs as links, code or bare file names, When the directory resolves in auto order, Then those docs follow the README in first-mention order"
bound = true

[[acceptance]]
text = "Given prose mentions of file names outside tables and lists, When references are collected, Then they are ignored unless they are links"
bound = true

[[acceptance]]
text = "Given an index referencing fewer than 3 docs or under half of its directory, When auto order runs, Then it is not treated as an index"
bound = true

[[acceptance]]
text = "Given docs in the directory the index does not list, When the book resolves, Then they follow the listed ones by path and are marked unlisted"
bound = true

[[acceptance]]
text = "Given references to missing files, to paths outside the book, to URLs or repeated, When the book resolves, Then they are skipped"
bound = true

[[acceptance]]
text = "Given an index that references a subdirectory or its README, When the book resolves, Then that subdirectory's resolved order is placed there"
bound = true
+++
