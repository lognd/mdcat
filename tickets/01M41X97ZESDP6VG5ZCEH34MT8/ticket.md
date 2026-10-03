+++
id = "01M41X97ZESDP6VG5ZCEH34MT8"
title = "Mark the repository and binary as the lognd fork"
type = "docs"
category = "in-progress"
priority = "medium"
points = 1
parent = "01M40SN4N73X0X52REQ2V7JZCX"
reporter = "lognd"
created = "2026-10-03T22:13:48Z"
updated = "2026-10-03T22:13:52Z"
persona = "the owner of the fork"
capability = "see at a glance that this is a fork, what it adds and how to update from upstream"
outcome_text = "I keep my features while merging upstream releases cleanly"
idempotency_key = "mdcat-fork-identity"
scope = ["FORK.md", "README.md", "Cargo.toml", "Cargo.lock", "tests/cli.rs"]

[[acceptance]]
text = "Given the repository, When FORK.md is read, Then it lists the fork's features, how to install the fork, how to merge upstream and which files conflict, where to report issues, and the release-tag warning"
bound = false

[[acceptance]]
text = "Given README.md, When it is opened, Then a short note at the top says this is the lognd fork and links FORK.md, and the rest of upstream's README is unchanged"
bound = false

[[acceptance]]
text = "Given the fork binary, When mdcat --version runs, Then the version carries the +lognd build suffix"
bound = false
+++
