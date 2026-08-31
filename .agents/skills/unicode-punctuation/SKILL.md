---
name: "unicode-punctuation"
description: >-
  Choose and write Unicode punctuation and typographic symbols in standalone
  documentation, ordinary code comments, and documentation comments. Use when
  creating or revising prose that may benefit from dashes, quotation marks,
  apostrophes, ellipses, section, copyright, trademark, or similar non-ASCII
  marks while respecting source safety, rendering, and machine-consumed text.
---

# Unicode punctuation

Use Unicode punctuation and typographic symbols in new or directly revised
human-readable prose when they are safe and semantically appropriate. Do not
normalize unrelated passages merely because they use different punctuation.

## Scope gate

Apply this skill's punctuation rules directly to first-party prose within the
task's scope. Do not inspect nearby precedent to decide whether these rules
apply.

If the target is inside a vendored fork, use `$work-in-vendored-forks` first and
apply the resolved upstream punctuation conventions within that fork. Do not
invoke the vendored-fork workflow merely because first-party code calls, wraps,
configures, or otherwise interfaces with a third-party dependency.

Compiler, formatter, linter, parser, renderer, and source-encoding constraints
still control whether and how the selected character can be represented.

## Decide how to encode the character

Before inserting literal non-ASCII text, apply `$unicode-source-safety` to the
exact target file and supported processing path.

- If the result is `Safe`, use the literal character when it is otherwise
  appropriate.
- If the result is `Unsafe` and the exact parsed documentation context defines
  an escape that renders the intended character there, use that escape.
- Otherwise, use conventional ASCII punctuation or rewrite the prose without the
  character.

Use a documentation escape only where the applicable parser interprets it. Never
put one where it will display literally, such as an ordinary source comment, a
verbatim region, or a code span. Treat a documentation comment as a parsed
context only when its actual documentation tool expands the escape at that exact
position.

## Choose punctuation by meaning

Choose each character for its actual prose meaning rather than as decoration.
Distinguish hyphens, minus signs, en dashes, and em dashes; distinguish straight
machine delimiters from prose quotation marks and apostrophes. Use ellipses,
`§`, `©`, `®`, `™`, and comparable symbols only when they convey their
conventional meaning in the sentence.

When an em dash is likely to be rendered in a monospaced font, surround it with
ordinary spaces: `word — word`. This normally includes ordinary source comments,
plain-text or terminal documentation, and monospaced rendered blocks. When an
escape renders the dash, put the spaces outside the escape. Do not infer
monospaced output solely because the source is edited in a monospaced editor. In
proportional prose, close the em dash against the surrounding words:
`word—word`.

Preserve machine-consumed or exact text, including code spans and blocks,
identifiers, commands, paths, URLs, regular expressions, protocol values,
directives, serialized data, fixtures, and quoted source text. Do not replace
their ASCII characters with typographic lookalikes.

When punctuation is part of a person's preferred name, defer to
`$represent-person-names` instead of normalizing it under this skill.

## Author and reviewer checks

When writing, re-read the raw source and rendered or displayed prose when
possible. Confirm that each character carries the intended meaning, is encoded
safely, and uses monospaced or proportional em-dash spacing as applicable.

During review, report conventional ASCII punctuation in new or directly revised
prose when the Unicode form is safe and semantically appropriate. Do not report
it solely because a Unicode alternative exists in unrelated prose, and do not
audit unchanged passages.
