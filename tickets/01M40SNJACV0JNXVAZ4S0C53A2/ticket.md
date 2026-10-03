+++
id = "01M40SNJACV0JNXVAZ4S0C53A2"
title = "Table of contents overlay for the book pager"
type = "story"
category = "in-progress"
priority = "medium"
points = 3
parent = "01M40SN4N73X0X52REQ2V7JZCX"
reporter = "lognd"
created = "2026-10-03T11:51:23Z"
updated = "2026-10-03T12:21:40Z"
persona = "a reader of long documentation sets"
capability = "open a list of all documents on demand and jump to one"
outcome_text = "I can find my place without the list ever being in the way"
idempotency_key = "mdcat-book-toc"
scope = ["src/book/**", "tests/**", "mdcat.1.adoc", "README.md", "changelog.d/**"]

[[links]]
kind = "blocked-by"
target = "01M40SNJ6CH7GVCDJE0EPP5C3X"

[[acceptance]]
text = "Given the pager, When it opens, Then no table of contents is shown"
bound = true

[[acceptance]]
text = "Given the pager, When Tab is pressed, Then a table of contents of the documents in reading order opens with the current one highlighted"
bound = true

[[acceptance]]
text = "Given the open table of contents, When Up/Down then Enter is pressed, Then the selected document is shown and the overlay closes"
bound = false

[[acceptance]]
text = "Given the open table of contents, When Tab or Esc is pressed, Then it closes leaving the current document unchanged"
bound = false
+++
