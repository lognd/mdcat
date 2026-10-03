#!/usr/bin/env bash
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# Test scripts/sync-upstream.sh against fake upstream releases, in a throwaway clone.
#
# Usage: scripts/test-sync-upstream.sh
#
# Builds an "upstream" from the commit the fork last merged, releases a version bump there that
# also touches README.md's first line and Cargo.lock, and checks that the script merges it with
# no help. Then releases a change to a line the fork also changed in src/main.rs and checks that
# the script stops and names that file.

set -euo pipefail

log() { printf 'test-sync-upstream: %s\n' "$*" >&2; }
fail() {
    log "FAIL: $*"
    exit 1
}

repo="$(git rev-parse --show-toplevel)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# The upstream commit the fork last merged: the newest commit of main that upstream also has.
base="$(git -C "$repo" merge-base HEAD upstream/main)"
log "fake upstream from $(git -C "$repo" rev-parse --short "$base")"

git clone -q "$repo" "$work/fork"
git clone -q "$repo" "$work/upstream"
fork="$work/fork"
upstream="$work/upstream"
git -C "$fork" checkout -q -B main "$(git -C "$repo" rev-parse HEAD)"
# Copy uncommitted versions of the script under test into the clone.
cp "$repo/scripts/sync-upstream.sh" "$fork/scripts/sync-upstream.sh"
git -C "$fork" add scripts/sync-upstream.sh
git -C "$fork" -c user.name=test -c user.email=test@example.com commit -q -m "test: script under test" || true
git -C "$upstream" checkout -q -B main "$base"
git -C "$fork" remote remove upstream 2>/dev/null || true
git -C "$fork" remote add upstream "$upstream"

commit_upstream() {
    git -C "$upstream" -c user.name=upstream -c user.email=upstream@example.com commit -q -am "$1"
}

log "case 1: an upstream release bumps the version and edits README.md and Cargo.lock"
old_version="$(sed -n 's/^version = "\([^"+]*\).*"$/\1/p' "$upstream/Cargo.toml" | head -1)"
sed -i "s/^version = \"$old_version\"$/version = \"99.0.0\"/" "$upstream/Cargo.toml"
sed -i "s/^pulldown-cmark-mdcat = { version = \"=$old_version\"/pulldown-cmark-mdcat = { version = \"=99.0.0\"/" "$upstream/Cargo.toml"
sed -i "s/^version = \"$old_version\"$/version = \"99.0.0\"/" "$upstream/pulldown-cmark-mdcat/Cargo.toml" 2>/dev/null || true
sed -i '/^name = "mdcat"$/{n;s/^version = .*/version = "99.0.0"/}' "$upstream/Cargo.lock"
sed -i '/^name = "pulldown-cmark-mdcat"$/{n;s/^version = .*/version = "99.0.0"/}' "$upstream/Cargo.lock"
sed -i '1s/.*/# mdcat, release 99/' "$upstream/README.md"
commit_upstream "Release 99.0.0"

(cd "$fork" && scripts/sync-upstream.sh --no-check) || fail "the script did not merge release 99"
grep -qx 'version = "99.0.0+lognd"' "$fork/Cargo.toml" || fail "Cargo.toml version: $(grep -m1 '^version' "$fork/Cargo.toml")"
head -1 "$fork/README.md" | grep -qxF '<!-- lognd-fork-note:start -->' || fail "the fork note is not on top of README.md"
grep -qxF '# mdcat, release 99' "$fork/README.md" || fail "upstream's README.md change is missing"
grep -q 'name = "ratatui"' "$fork/Cargo.lock" || fail "the fork's dependencies are missing from Cargo.lock"
[[ -z "$(git -C "$fork" status --porcelain --untracked-files=no)" ]] || fail "the merge left changes behind"
git -C "$fork" rev-parse -q --verify HEAD^2 >/dev/null || fail "no merge commit"
(cd "$fork" && scripts/sync-upstream.sh --no-check) || fail "a second run is not a no-op"
log "case 1 passed"

log "case 2: an upstream change conflicts with the fork in src/main.rs"
sed -i 's|^mod picker;$|mod picker; // upstream touched this line|' "$upstream/src/main.rs"
commit_upstream "Touch the line the fork changed"
if (cd "$fork" && scripts/sync-upstream.sh --no-check 2>"$work/stderr"); then
    fail "the script merged a conflict it cannot resolve"
fi
grep -q 'src/main.rs' "$work/stderr" || fail "src/main.rs is not listed: $(cat "$work/stderr")"
git -C "$fork" rev-parse -q --verify MERGE_HEAD >/dev/null || fail "the merge is not left in progress"
git -C "$fork" checkout -q --ours -- src/main.rs
git -C "$fork" add src/main.rs
(cd "$fork" && scripts/sync-upstream.sh --continue --no-check) || fail "--continue did not finish the merge"
git -C "$fork" rev-parse -q --verify MERGE_HEAD >/dev/null && fail "the merge is still in progress"
log "case 2 passed"
log "all passed"
