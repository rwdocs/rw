---
name: writing-changelog
description: Writes and audits entries in rw's CHANGELOG.md. Use this whenever you are about to add, edit, or review a changelog entry — including the routine "update CHANGELOG.md after code changes" step in CLAUDE.md — and whenever cutting a release, since cargo-dist pastes the changelog straight into the GitHub release body. Also use when deciding whether a change deserves an entry at all, when asked to shorten, clean up, or restructure the changelog, and when the user mentions release notes, the [Unreleased] section, or what to tell users about a change.
---

# Writing rw's changelog

cargo-dist pastes the matching `CHANGELOG.md` section verbatim into the GitHub
release body, so the changelog *is* the release notes. You are writing for
someone who has never seen rw's source and never will, scanning to answer one
question: **does this affect me, and do I have to do anything?**

## Does this change get an entry at all?

Ask which artifact the change is observable through.

| Observable through | Entry? |
|---|---|
| `rw` CLI behaviour or output | yes |
| the site `rw serve` renders | yes |
| `@rwdocs/core`'s napi bindings | yes |
| `@rwdocs/viewer` behaviour or exports | yes |
| only another crate in this workspace | **no** |

"Public" in the Rust sense is not rw's user surface — the crates aren't published
for third-party consumption, so removing a `pub` item from `rw-renderer` is an
internal refactor however much it looks like a breaking change.

Also skip refactors, clippy cleanups, test and bench changes, and dependency
bumps with no visible effect. A `perf:` commit earns an entry only when a user
would notice ("startup scan is ~3x faster on large sites"), not for a
micro-optimization.

## Cut the mechanism, keep the consequence

Open with one sentence naming the user-visible change, led by the noun the reader
recognizes — the flag, the command, the thing on screen — not by "Fixed", which
the heading already says.

Everything after that first sentence has to earn its place. Entries inflate
because you write them right after the change, when the mechanism is the most
vivid thing in your head: you just spent an hour working out that the indent
pattern swallowed a newline, and it feels like the point. It isn't. The reader
has never seen that code.

**Test: delete every sentence that only makes sense to someone who has read the
source. If the entry still says what changed, those sentences were mechanism.**

Tells that you are writing mechanism:

- Naming code the reader can't see — "a pattern that also swallowed the preceding
  newline"
- Narrating the investigation — "It was also broken in a way nobody noticed…"
- Arguing the change to a reviewer — "It never earned the configuration…"
- Explaining *why* something is unaffected instead of just saying it. **Keep the
  claim, cut the because**: "sites using the default `source_dir` are unaffected"
  earns its seven words; the clause explaining that two directories coincide is
  twenty more that change nobody's decision.

### What earns extra words

Exactly four things:

- The reader has to **do** something — edit config, change a call, update a selector
- The reader would reasonably worry and can be reassured cheaply — "no cache
  clearing needed", "SVG diagrams were never affected"
- A breaking change needs its **replacement** named
- The change is a headline, and one clause of symptom makes it land — "they were
  being shrunk to half size, leaving labels close to unreadable"

Nothing else. Not why the bug happened, not what you considered and rejected, not
an exhaustive list of configurations that are fine.

Word count is a tripwire, not a target. Count each finished entry. Over 40 words,
put every sentence against the four above; anything that doesn't match is
mechanism and goes. If they all match — rare, usually a big change with real
migration consequences — 50 is fine. What you must not do is *compress*: turning
a 130-word explanation into 60 denser words keeps every sentence that should have
been cut and makes the survivors harder to read.

### Name who acts

Passive voice hides in plain sight, because the sentence still reads fine. "They
were being shrunk to half size" never says what was shrinking them: the browser?
Kroki? rw? With a CLI, a napi binding and a viewer in play, the actor *is* scope,
and scope is what a reader scans for.

- "It was parsed but never read" → "rw parsed it but never read it"
- "cached diagrams are corrected" → "rw corrects cached diagrams"

Naming the actor is usually free. When it costs a word or two it is buying scope,
which is worth it — but it is not licence to spend elsewhere, and the 40-word
tripwire still applies. One exception: when the actor is the reader, "no cache
clearing is needed" beats "you do not need to clear the cache".

Two smaller habits, same pass. An em dash used as a general connector is usually
a colon, a semicolon, or a full stop, sometimes nothing at all: "the `vars` field
is gone — from `meta.yaml`" is just "gone from `meta.yaml`". And watch for pairs
saying one thing twice, like "consistently uses `X/index.md` everywhere".

Stop there. "Vary your rhythm", "give every sentence a human subject" and "never
use an em dash" are tuned for prose read start to finish. A changelog is scanned:
entries all closing on "are unaffected" is a feature, the same signal in the same
place telling a reader to stop, and an entry's subject is usually the software's
behaviour, because that behaviour is the news.

### Where the depth goes

Real explanation belongs in `docs/` — `configuration.md`, `diagrams.md`,
`metadata.md`, `embedding.md`, `confluence.md`, `comment-cli.md`,
`status-badges.md` — and the entry links to it. That link is what makes a short
entry safe: nothing is lost, it just sits one level down instead of in the
reader's way. If a change needs explaining and no doc covers it, the docs need a
paragraph, not the changelog. Depth aimed at reviewers goes in the commit message
and the PR, where reviewers actually look.

## Make sure it's true before you make it short

A short claim gets read and acted on with more confidence than a long one, so a
compressed wrong entry does more damage than the verbose wrong entry it replaced.

What goes wrong is almost always **scope** — "affects X", "Y was never affected"
— because scope gets written from the commit message rather than the code. Two
real examples from this changelog: an entry claimed an S3 manifest crash "affects
`rw serve`", but `rw-server` has no dependency on `rw-storage-s3` at all; and the
`dpi` removal entry said diagrams "still render exactly as before" while two
entries below it, from the same commit, said they now render twice as large.

So before finalizing: confirm any named surface actually reaches the code path (a
grep, or the crate's `Cargo.toml`, usually settles it); check any "unaffected"
claim, since readers use those to skip the entry; and read the neighbouring
entries, since this is the first time they are seen side by side.

## Examples from rw's own history

**Nearly all mechanism.** Before, 130 words — the fix, then the indent pattern,
the re-application, which PlantUML constructs might reveal it, the CRLF variant.
After, 24:

> A PlantUML `!include` preceded by a blank line no longer double-spaces every
> line of the included file. Indentation on an indented `!include` is preserved.

**A breaking change that argues its case.** Before, 198 words, three sentences of
which justified the removal to a reviewer. After, 41:

> **Breaking (pre-1.0):** the diagram `dpi` setting is gone — `[diagrams] dpi` in
> `rw.toml`, `--dpi` on `rw confluence render`, and `diagrams.dpi` in
> `@rwdocs/core`'s `createSite()`. Diagrams render exactly as before, and an
> `rw.toml` that still sets it keeps loading.

The reader needs four things: it's gone, here's everywhere it was, nothing you
see changes, your config still loads. The "why remove it" belongs in the PR.

**Enumerating every case.** Before, 180 words on the `README.md` homepage
fallback. After, 35 — the "unaffected" claim stays because it lets most readers
stop; what went is why they're unaffected and what used to happen instead:

> A site whose `docs.source_dir` is nested, absolute, or the project root itself
> (`"."`) now finds its `README.md` homepage at the project root. Sites using the
> default `source_dir = "docs"` are unaffected.

rw wrote entries like this for its first twenty releases. Mean entry length has
gone 7 → 16 → 31 → 58 → 128 words since February. The drift is reversible.

## Sections and conventions

- Keep a Changelog order: Added, Changed, Deprecated, Removed, Fixed, Security.
  Write only the sections you need.
- New entries go under `## [Unreleased]`. Read the whole section first and append
  to the existing subsection — never open a second `### Fixed`.
- Released version sections are immutable. They are already published as GitHub
  release bodies; editing one makes the two disagree permanently.
- Breaking changes keep rw's inline prefix — `- **Breaking (pre-1.0):** …` inside
  Added, Changed, or Removed. No separate section.
- rw's commits carry no PR or issue numbers, so entries carry no attribution
  links. Don't invent them.

## Cutting a release: audit, then curate

Entries written one at a time drift — they miss commits, and they inflate.

**1. Audit coverage.**

```bash
git tag --sort=-version:refname | head -1
git log <tag>..HEAD --oneline
```

Walk every commit. `feat:` and `fix:` nearly always need an entry; `refactor:`,
`chore:`, `test:`, `bench:`, `docs:` nearly never do; `perf:` depends on whether
a user would notice. Use `git show <hash> --stat` when the subject isn't enough.

One trap: **a bug introduced and fixed within the same unreleased cycle nets to
zero.** Users upgrade from the last tag, so they never saw it. A `fix:` commit is
not evidence the bug shipped — check the behaviour at the last tag
(`git show <tag>:<file>`) before writing the entry.

**2. Audit shape and accuracy.** Reread every `[Unreleased]` entry against the
rules above. This is where mechanism gets caught, and where contradictions
between entries from different commits surface.

**3. Check that removals landed everywhere.** For each `Removed` entry, confirm
the thing is gone from `rw.toml.example`, `docs/`, and `README.md`. Shipping an
example config that still advertises a just-deleted setting is a worse first
impression than the removal itself.

**4. Propose the highlight reel.** Insert `### New Features` as the *first*
subsection of `## [Unreleased]`:

```markdown
### New Features

- **Project-directory targeting** — `rw serve --project-dir <dir>` points rw at a
  project you are not in, rooting configuration, docs, and `.rw/` at that
  directory. See [Configuration](docs/configuration.md).
```

One to three items: bolded lead phrase, em dash, one sentence, doc link where one
exists. What you'd say if someone asked "what's in this release?" — the headline,
not a copy of every `Added` line.

**Propose them and wait for confirmation before writing.** Which items deserve
top billing is a judgement about what the project wants to be known for, and that
call belongs to the user. Every item stays in its normal section too; the reel is
a second view, not a replacement.
