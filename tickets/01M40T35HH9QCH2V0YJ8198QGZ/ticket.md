+++
id = "01M40T35HH9QCH2V0YJ8198QGZ"
title = "Report resource handler creation failure instead of unwrapping"
type = "chore"
category = "triage"
priority = "low"
reporter = "lognd"
created = "2026-10-03T11:58:49Z"
updated = "2026-10-03T11:58:49Z"
idempotency_key = "mdcat-upstream-todo-resource-handler"
scope = ["src/main.rs"]
+++

Upstream's main.rs unwraps create_resource_handler with a bare TODO ("Handle this error properly"); a curl client build failure panics instead of exiting with an error message. Owned here so the TODO is tracked.
