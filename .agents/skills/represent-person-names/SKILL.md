---
name: "represent-person-names"
description: >-
  Represent the names of people faithfully in standalone documentation, ordinary
  source-code comments, and documentation comments. Use this skill when writing
  or reviewing prose that names a person, especially when choosing preferred
  orthography, letters and diacritics, native-script and Latin-script forms,
  transliteration, literal Unicode, or a documentation-format escape.
---

# Represent person names

Write each person's name faithfully without guessing. Apply these rules to
human-readable prose in standalone documentation, ordinary comments, and
documentation comments. Preserve machine-consumed names and identifiers unless
the task explicitly requires changing them.

## Scope gate

Apply this skill's faithful-name rules directly to first-party prose within the
task's scope. Do not inspect nearby precedent to decide whether these rules
apply.

If the target is inside a vendored fork, use `$work-in-vendored-forks` first and
apply the resolved upstream name-representation conventions within that fork. Do
not invoke the vendored-fork workflow merely because first-party code calls,
wraps, configures, or otherwise interfaces with a third-party dependency.

Compiler, parser, documentation-generator, renderer, and source-encoding
constraints determine how to represent the faithful name safely; they do not
justify silently changing the person's name.

## Choose the faithful name

- Use the person's preferred orthography when it is known. Preserve its genuine
  letters, diacritics, and other orthographic marks.
- When the preferred spelling is unknown, retain the form supplied by the user,
  a cited source, or existing project documentation.
- Never guess a spelling, add or remove diacritics, expand initials, or invent a
  transliteration. Do not create an ASCII form by mechanically stripping marks.
- Do not substitute typographic presentation ligatures such as `ﬁ` or `ﬂ`.
  Preserve genuine orthographic letters such as `Æ` when they are part of the
  person's name.

For a name normally written in a non-Latin script:

1.  Write `preferred-script form (Latin-script form)` at the first substantive
    prose mention in the file.
2.  Use the Latin-script form consistently in later prose mentions.
3.  Prefer the person's own Latin-script spelling when available. Otherwise, use
    one established transliteration consistently.

Do not treat a byline, citation metadata, bibliography entry, index key, or code
identifier as a substantive prose mention. Preserve a supplied byline exactly
unless the task explicitly requires changing it. Follow the applicable citation
style and source metadata without silently normalizing structured author fields.
Preserve the structure and exact machine-consumed spelling of citation keys,
identifiers, paths, URLs, and similar fields. Do not rewrite an identifier to
match a name used in surrounding prose.

## Choose literal text, an escape, or a safe form

After choosing the faithful displayed form, handle each non-ASCII character as
follows:

1.  Apply `$unicode-source-safety` to the exact target file and every supported
    source-processing path.
2.  If the result is `Safe`, write the preferred form using literal Unicode.
3.  If the result is `Unsafe` and the exact parsed documentation format defines
    an escape that is valid in this location and renders the intended character,
    use that escape.
4.  Otherwise, use a known preferred or established form that is safe in the
    target source path. If no such form is known, ask for one rather than
    manufacturing it.

Use documentation escapes only in documentation that actually parses them. Do
not put them in ordinary comments, code or verbatim regions, identifiers, or
other contexts where they would remain literal or change machine-consumed text.

## Author and reviewer checks

When writing, re-read the rendered or displayed prose when possible. Confirm
that the name has the intended spelling, that a non-Latin name follows the
first-mention rule, and that no source representation accidentally became part
of the visible name.

When reviewing in-scope prose, report guessed or altered spelling, lost genuine
orthographic marks, invented transliteration, incorrect non-Latin first-mention
form, unsafe source encoding, an escape the target does not parse, or a change
to machine-consumed text. Do not audit unrelated prose.
