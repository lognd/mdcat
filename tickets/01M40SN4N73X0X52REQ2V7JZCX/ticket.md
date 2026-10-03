+++
id = "01M40SN4N73X0X52REQ2V7JZCX"
title = "Book mode: read a set of markdown docs in order inside mdcat"
type = "epic"
category = "todo"
priority = "medium"
reporter = "lognd"
created = "2026-10-03T11:51:09Z"
updated = "2026-10-03T11:51:09Z"
idempotency_key = "mdcat-book-epic"
+++

Read long documentation sets in the terminal like a book: in reading order, file to file, without leaving the pager. Owner decisions 2026-10-03: --book/-b flag, a directory argument implies book mode; non-TTY prints all docs in order; inline images when the terminal supports them; status line shows title, n/N, order source, scroll percent and a key hint.
