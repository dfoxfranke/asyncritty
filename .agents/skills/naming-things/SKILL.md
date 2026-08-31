---
name: "naming-things"
description: >-
  Use this skill when writing or reviewing code that introduces, changes, or
  evaluates identifier names, especially names for functions, methods, and
  predicates. Choose names quickly, defer to established project conventions,
  and use part-of-speech guidance only as a fallback.
---

# Naming things

Apply these rules to names already within the task's scope. Do not broaden the
task into a naming cleanup or start a review loop to choose among reasonable
alternatives.

- Follow applicable explicit rules and a clear, well-established convention in
  analogous code before this skill. Do not conduct a broad naming audit merely
  to find a convention.
- Prefer predicates that read as properties or adjectives. Omit a copula when
  the bare adjective is clear; retain it to avoid ambiguity, as in `is_empty`.
- Give command-like functions verbs and query-like functions names that describe
  their results, usually nouns: `invert` for in-place work and `inverse` for a
  returned value. Classify by how the operation is understood, not by technical
  purity; established names such as `parse`, `contains`, and `encrypt` remain
  natural.
- Treat asynchronous or blocking I/O operations as commands, even when they
  only fetch data.
- When using a transitive verb, omit its object if call-site context makes it
  obvious; include it when the API needs to distinguish different objects.

Treat these as defaults, not grammar laws. If strict application produces an
awkward name, choose a natural, convention-consistent name and move on.
