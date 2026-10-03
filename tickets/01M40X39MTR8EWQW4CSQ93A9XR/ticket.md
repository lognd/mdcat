+++
id = "01M40X39MTR8EWQW4CSQ93A9XR"
title = "Show (END) and the next document's title at the bottom of a document"
type = "story"
category = "todo"
priority = "high"
points = 1
parent = "01M40SN4N73X0X52REQ2V7JZCX"
reporter = "lognd"
created = "2026-10-03T12:51:19Z"
updated = "2026-10-03T12:51:19Z"
persona = "a reader of long documentation sets"
capability = "see when I have reached the end of a document and what Right opens next"
outcome_text = "I do not scroll on past the end wondering if there is more"
idempotency_key = "mdcat-book-end-marker"
scope = ["src/book/pager.rs", "mdcat.1.adoc", "README.md", "changelog.d/**"]

[[acceptance]]
text = "Given the last line of a document is visible, When the status line draws, Then it shows (END) instead of the scroll percent"
bound = false

[[acceptance]]
text = "Given the end of a document that is not the last, When the status line draws, Then (END) is followed by -> and the next document's title"
bound = false

[[acceptance]]
text = "Given the last document of the book, When its end is visible, Then the status line shows (END) with no next title"
bound = false
+++
