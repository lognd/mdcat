# lognd/mdcat: a fork of mdcat

This repository is [lognd/mdcat](https://github.com/lognd/mdcat), a permanent fork of
[BIRSAx2/mdcat](https://github.com/BIRSAx2/mdcat) (itself the maintained continuation of the
archived [swsnr/mdcat](https://github.com/swsnr/mdcat)). It adds features for reading long sets of
documentation in the terminal, and it follows upstream: every upstream release is merged in.
Nothing here is sent upstream as a pull request.

Everything in [README.md](README.md) still applies; this page covers only what the fork adds and
how the fork is kept up to date. The fork's binary reports itself with a `+lognd` version suffix,
for example `mdcat 2.18.0+lognd`.

## What the fork adds

### Reading a book

Point `mdcat` at a directory, or pass `--book` (`-b`) with an [mdBook]-style `SUMMARY.md` or an
index file, to read a whole set of documents in order:

```console
$ mdcat docs                  # read docs/ in its detected reading order
$ mdcat --book docs/SUMMARY.md
$ mdcat --order path docs     # ignore SUMMARY.md, indexes and weights; go by name
```

Each directory is ordered on its own, by the most explicit source it has:

1. the links in its `SUMMARY.md`, depth-first;
2. else the order its `README.md` (or `index.md`) lists the directory's documents in tables or
   lists, if it names most of them, with the ones it leaves out appended as unlisted;
3. else its `README.md` first, then documents by frontmatter weight (`nav_order`, `weight` or
   `sidebar_position`);
4. then the rest by name, in natural order (`ch2.md` before `ch10.md`).

`--order auto|summary|index|frontmatter|path` (or `defaults.book_order` in the config file)
chooses one source up front. Piped or redirected, every document is rendered in reading order.

### The book pager

On a terminal a book opens in a built-in pager:

| Key | Action |
|---|---|
| <kbd>Left</kbd>/<kbd>Right</kbd>, <kbd>h</kbd>/<kbd>l</kbd> | previous or next document |
| <kbd>Up</kbd>/<kbd>Down</kbd>, <kbd>PgUp</kbd>/<kbd>PgDn</kbd>, <kbd>Space</kbd> | scroll |
| <kbd>g</kbd>/<kbd>G</kbd> | top or bottom of the document |
| <kbd>Tab</kbd> | table of contents: arrows and <kbd>Enter</kbd> jump, <kbd>Tab</kbd>/<kbd>Esc</kbd> close |
| <kbd>o</kbd> | switch to the next reading order that applies |
| <kbd>q</kbd> | quit |

Each document remembers where you left it. A status line shows the document's title, its position
in the book, where the order came from, and how far down you are, with `(END) -> next title` at
the bottom of a document. Inline images render on kitty, iTerm2 and sixel terminals.

The full reference is the `Reading a book` section of `man 1 mdcat` ([mdcat.1.adoc](mdcat.1.adoc)).

[mdBook]: https://rust-lang.github.io/mdBook/format/summary.html

## Installing the fork

```console
$ cargo install --git https://github.com/lognd/mdcat mdcat
```

or, from a checkout, `cargo install --path . --locked`. The fork is not published to crates.io;
`cargo install mdcat` installs upstream.

## Reporting issues

Problems with book mode, the book pager or anything else this page lists belong in
[lognd/mdcat issues](https://github.com/lognd/mdcat/issues). Everything else is upstream's code:
check whether upstream mdcat has the same problem, and if so report it to
[BIRSAx2/mdcat](https://github.com/BIRSAx2/mdcat/issues).

## Keeping up with upstream

The `upstream` remote points at BIRSAx2/mdcat. To merge an upstream release:

```console
$ scripts/sync-upstream.sh              # merge upstream/main
$ scripts/sync-upstream.sh mdcat-2.19.0 # or a specific upstream tag
$ git push origin main
```

The script fetches upstream, merges it into `main` (a merge, never a rebase: `main` is
published), resolves the conflicts the fork expects, then builds, lints and tests before it commits
the merge. It never pushes. It stops and leaves the merge in progress, listing the files, when a
conflict needs a person; resolve them, `git add` them, and run it again with `--continue`.

The conflicts it resolves on its own:

| File | Conflict | Resolution |
|---|---|---|
| `Cargo.toml` | upstream bumped `version` | upstream's version plus `+lognd` |
| `Cargo.lock` | any | upstream's lockfile, then cargo adds what the fork needs, upgrading nothing |
| `README.md` | the fork note at the top | upstream's README with the note put back on top |
| anything | a conflict resolved by hand before | replayed by `git rerere`, which the script enables |

Where to expect the rest: `src/main.rs` and `src/args.rs` (the fork adds a few lines where book
mode hooks in, and its options at the end of `CommonArgs`), `src/config.rs` (`book_order` at the
end of `Defaults`), and `mdcat.1.adoc` (the book sections). Everything else the fork adds lives in
files upstream does not have: `src/book/`, `tests/book.rs`, `tests/book/`, this page.

### Release tags

Fetching upstream brings its `mdcat-*` release tags along. Upstream's
`.github/workflows/release.yml` builds a release for every pushed `mdcat-*` tag, so it is disabled
on this fork; never push upstream's tags anyway (no `git push --tags`).

## How this fork is worked

Work is tracked with frob: tickets live in `tickets/`, user-facing
changes are one changelog fragment each in `changelog.d/` (upstream's `CHANGELOG.md` stays
upstream's, so it never conflicts), and `frob.toml` holds the settings. New code follows upstream's
style, tooling and licence (MPL-2.0), and goes in new files where it can, so that upstream merges
stay clean.
