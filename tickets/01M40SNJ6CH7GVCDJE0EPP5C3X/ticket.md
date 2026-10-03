+++
id = "01M40SNJ6CH7GVCDJE0EPP5C3X"
title = "Built-in book pager with file-to-file navigation and status line"
type = "story"
category = "in-progress"
priority = "high"
points = 5
parent = "01M40SN4N73X0X52REQ2V7JZCX"
reporter = "lognd"
created = "2026-10-03T11:51:23Z"
updated = "2026-10-03T12:11:38Z"
persona = "a reader of long documentation sets"
capability = "page through the book and move between files with Left/Right"
outcome_text = "moving to the next document is a pager action, not a new command"
idempotency_key = "mdcat-book-pager"
scope = ["src/book/**", "src/main.rs", "Cargo.toml", "Cargo.lock", "tests/**", "mdcat.1.adoc", "README.md", "changelog.d/**"]

[[links]]
kind = "blocked-by"
target = "01M40SNJ2GJH6Q0JXDWTJX99KA"

[[acceptance]]
text = "Given book mode on a terminal, When Right/l or Left/h is pressed, Then the next or previous document in reading order is shown, staying put at either end"
bound = true

[[acceptance]]
text = "Given a document, When Up/Down, PgUp/PgDn, Space, g or G is pressed, Then the view scrolls by a line, a page, a page, to the top or to the bottom, clamped to the document"
bound = true

[[acceptance]]
text = "Given a scrolled document, When the reader moves to another file and back, Then the earlier scroll position is restored"
bound = true

[[acceptance]]
text = "Given the pager is open, When it draws, Then the bottom line shows the title, n/N, the order source, the scroll percent and the key hint"
bound = true

[[acceptance]]
text = "Given the terminal is resized, When the pager redraws, Then documents re-render at the new width; and q quits restoring the terminal"
bound = false
+++
