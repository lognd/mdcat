#!/usr/bin/env bash
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# Merge an upstream mdcat release into the lognd fork, resolving the conflicts the fork expects.
#
# Usage: scripts/sync-upstream.sh [--continue] [--no-check] [REF]
#
#   REF         the upstream ref to merge, default upstream/main; a tag like mdcat-2.19.0 works
#   --continue  finish a merge this script left in progress, after resolving and `git add`ing the
#               remaining conflicts by hand
#   --no-check  commit without building, linting and testing first (for testing this script)
#
# Resolves on its own: the version line in Cargo.toml (upstream's version plus +lognd), Cargo.lock
# (upstream's, then cargo adds what the fork needs), the fork note at the top of README.md
# (upstream's README with the note put back), and anything git rerere has seen resolved before.
# Leaves every other conflict to a person, listing the files. Never pushes. See FORK.md.

set -euo pipefail

readonly UPSTREAM_URL="https://github.com/BIRSAx2/mdcat"
readonly VERSION_SUFFIX="+lognd"
readonly NOTE_START="<!-- lognd-fork-note:start -->"
readonly NOTE_END="<!-- lognd-fork-note:end -->"

log() { printf 'sync-upstream: %s\n' "$*" >&2; }
die() {
    log "error: $*"
    exit 1
}

ref="upstream/main"
resume=false
check=true
while (($#)); do
    case "$1" in
    --continue) resume=true ;;
    --no-check) check=false ;;
    -h | --help)
        sed -n '6,19p' "$0" | sed 's/^# \{0,1\}//'
        exit 0
        ;;
    -*) die "unknown option $1" ;;
    *) ref="$1" ;;
    esac
    shift
done

cd "$(git rev-parse --show-toplevel)"

conflicted() { git diff --name-only --diff-filter=U; }
is_conflicted() { conflicted | grep -qxF "$1"; }
merging() { git rev-parse -q --verify MERGE_HEAD >/dev/null; }

# Replay conflict resolutions made by hand in earlier merges, and stage what they resolve.
git config rerere.enabled true
git config rerere.autoupdate true

start_merge() {
    [[ "$(git branch --show-current)" == main ]] || die "run this on main"
    merging && die "a merge is in progress; finish it with --continue or abort it"
    git diff --quiet && git diff --cached --quiet || die "commit or stash your changes first"
    if ! git remote get-url upstream >/dev/null 2>&1; then
        log "adding the upstream remote $UPSTREAM_URL"
        git remote add upstream "$UPSTREAM_URL"
    fi
    log "fetching upstream"
    git fetch --tags upstream
    git rev-parse -q --verify "$ref^{commit}" >/dev/null || die "no such ref: $ref"
    if git merge-base --is-ancestor "$ref" HEAD; then
        log "already up to date with $ref"
        exit 0
    fi
    log "merging $ref"
    if ! git merge --no-ff --no-commit "$ref"; then
        merging || die "git merge failed before it could record a merge"
        log "the merge conflicts in: $(conflicted | tr '\n' ' ')"
    fi
}

# Resolve conflict hunks in Cargo.toml that only differ in the package version line: take
# upstream's version and add the fork's suffix. Other hunks keep their markers.
resolve_version() {
    is_conflicted Cargo.toml || return 0
    local resolved
    resolved="$(mktemp)"
    awk -v suffix="$VERSION_SUFFIX" '
        function is_version(line) { return line ~ /^version = "[^"]*"$/ }
        function flush_hunk(    version) {
            if (n_ours == 1 && n_theirs == 1 && is_version(ours[1]) && is_version(theirs[1])) {
                version = theirs[1]
                sub(/^version = "/, "", version); sub(/"$/, "", version); sub(/\+.*$/, "", version)
                print "version = \"" version suffix "\""
            } else {
                print open_marker
                for (i = 1; i <= n_ours; i++) print ours[i]
                if (base_marker != "") {
                    print base_marker
                    for (i = 1; i <= n_base; i++) print base[i]
                }
                print "======="
                for (i = 1; i <= n_theirs; i++) print theirs[i]
                print $0
            }
        }
        /^<<<<<<< / { state = "ours"; open_marker = $0; base_marker = ""; n_ours = n_base = n_theirs = 0; next }
        /^\|\|\|\|\|\|\| / && state == "ours" { state = "base"; base_marker = $0; next }
        /^=======$/ && (state == "ours" || state == "base") { state = "theirs"; next }
        /^>>>>>>> / && state == "theirs" { flush_hunk(); state = ""; next }
        state == "ours" { ours[++n_ours] = $0; next }
        state == "base" { base[++n_base] = $0; next }
        state == "theirs" { theirs[++n_theirs] = $0; next }
        { print }
    ' Cargo.toml >"$resolved"
    cat "$resolved" >Cargo.toml
    rm -f "$resolved"
    if grep -q '^<<<<<<< ' Cargo.toml; then
        log "Cargo.toml has conflicts besides the version line"
    else
        git add Cargo.toml
        log "resolved Cargo.toml: $(grep -m1 '^version = ' Cargo.toml)"
    fi
}

# Take upstream's README.md and put the fork note back on top. Safe because the note is the
# fork's only change to README.md; the fork's own documentation lives in FORK.md.
resolve_readme() {
    is_conflicted README.md || return 0
    local note
    note="$(git show HEAD:README.md | sed -n "/^$NOTE_START\$/,/^$NOTE_END\$/p")"
    [[ -n "$note" ]] || {
        log "README.md on main has no fork note; leaving its conflict to you"
        return 0
    }
    git checkout --theirs -- README.md
    if ! grep -qxF "$NOTE_START" README.md; then
        { printf '%s\n\n' "$note"; cat README.md; } >README.md.sync-upstream
        mv README.md.sync-upstream README.md
    fi
    git add README.md
    log "resolved README.md: upstream's text with the fork note on top"
}

# Take upstream's Cargo.lock when it conflicts, then let cargo bring the lockfile in line with
# the merged manifests (the fork's version and dependencies), resolving only what is missing.
resolve_lockfile() {
    if is_conflicted Cargo.lock; then
        git checkout --theirs -- Cargo.lock
        log "took upstream's Cargo.lock"
    fi
    if grep -q '^<<<<<<< ' Cargo.toml; then
        return 0
    fi
    cargo metadata --format-version 1 >/dev/null
    git add Cargo.lock
}

if $resume; then
    merging || die "no merge in progress to continue"
else
    start_merge
fi

resolve_version
resolve_readme
resolve_lockfile

remaining="$(conflicted)"
if [[ -n "$remaining" ]]; then
    log "these files still conflict; resolve them, git add them, then run"
    log "scripts/sync-upstream.sh --continue (or git merge --abort to give up):"
    printf '  %s\n' $remaining >&2
    exit 1
fi

if $check; then
    log "building, linting and testing the merge"
    cargo build --workspace ||
        die "the build fails; the merge is staged but not committed (fix, git add, --continue)"
    cargo clippy --workspace --all-targets -- -D warnings ||
        die "clippy fails; the merge is staged but not committed (fix, git add, --continue)"
    cargo test --workspace ||
        die "tests fail; the merge is staged but not committed (fix, git add, --continue)"
fi

git commit --no-edit
log "merged $ref"
log "review it with git log -1 --stat, then publish with git push origin main"
