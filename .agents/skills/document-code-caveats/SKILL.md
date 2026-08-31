---
name: "document-code-caveats"
description: >-
  Use maintainer-facing source comments to document exceptional code
  caveats. Use when a task must introduce or retain a workaround or
  compatibility kludge, deliberately leave incomplete or defective code for
  later, implement or review an inherently complex algorithm that code alone
  cannot explain, or explicitly add or revise WORKAROUND, TODO, or FIXME
  comments. Do not use for generic comment writing or review, ordinary bug
  fixes that will be completed now, API documentation, or Rust SAFETY comments.
---

# Document code caveats

Preserve maintainer knowledge that the code cannot communicate adequately on its
own. Explain the exceptional constraint or reasoning without narrating the
implementation.

## Scope gate

Apply this skill's requirements directly to first-party code and comments within
the task's scope. Do not inspect nearby precedent to decide whether these
requirements apply.

If the target is inside a vendored fork, use `$work-in-vendored-forks` first and
apply the resolved upstream comment conventions within that fork. Do not invoke
the vendored-fork workflow merely because first-party code calls, wraps,
configures, or otherwise interfaces with a third-party dependency.

Preserve syntax required by compilers, linters, generators, or other tools.
Within the syntax available for ordinary maintainer comments, use the tags in
this skill exactly as specified.

## Keep ordinary comments exceptional

Outside documentation comments, safety arguments, and mechanically significant
comments such as license notices and tool directives, use ordinary prose
comments mainly for the categories in this skill. Treat these categories as the
usual justifications, not as an exhaustive ban on every other comment.

Prefer clearer names, types, control flow, and decomposition whenever they can
make the code understandable. Do not translate expressions or statements into
prose. Record the non-obvious reason, invariant, external constraint, or future
condition that a maintainer needs to preserve.

Review only comments added, changed, or directly implicated by the task. Do not
expand a targeted task into an audit of unrelated narrative comments, even in a
file already being modified.

## Explain irreducible complexity

Comment an algorithm when it remains inherently difficult to understand after
making the implementation as clear as the task reasonably allows.

Explain the algorithm or strategy, the invariants or relationships on which it
depends, and any non-obvious correspondence between the exposition and the code.
Do not walk through operations that the code already states clearly.

Cite relevant academic literature, specifications, or other primary references
when the implementation follows or adapts them. Give enough information to
identify the source and, when practical, link directly to it. State material
departures from the cited method rather than implying that the reference proves
behavior the implementation does not share.

## Document workarounds and kludges

Treat code as a workaround when it intentionally departs from the preferred
implementation because of a known bug, compatibility requirement, or other
concrete constraint.

Place `WORKAROUND:` at the start of the comment. State:

- the exact constraint that makes the straightforward implementation unsuitable;
- the affected dependency, platform, version, or condition when relevant;
- why the workaround addresses that constraint; and
- what must become true before the workaround can be removed.

Cite the controlling issue, bug ticket, or authoritative reference. Keep enough
context in the source that a maintainer can understand the constraint without
depending entirely on the link.

When no citable record exists, do not invent one or create an external issue
without authorization. Instead, state the concrete triggering condition and a
testable removal condition in the comment.

Do not label an arbitrary preference or unexplained piece of awkward code a
workaround. The comment must identify the constraint that requires the kludge.

## Mark deferred defects and incomplete work

Use `TODO:` for work that is knowingly incomplete and remains to be done. Use
`FIXME:` for a known defect that remains in place.

State what remains incomplete or incorrect and what blocks completing or fixing
it immediately. Cite an existing tracking issue when one supplies useful
context, but keep the essential statement understandable locally.

If the issue is unblocked and can be completed within the task, complete it
instead of leaving a marker. When an explicit instruction requires a draft or
stub to remain incomplete and there is no independent blocker, state the
remaining work without inventing an explanation for why it is blocked.

Do not turn speculative improvements or vague dissatisfaction into TODOs. The
marker should record concrete unfinished behavior or a concrete known problem.

## Use related skills for other comment contracts

Use `$api-documentation-quality` and any applicable language profile for API and
documentation comments. For Rust, also use `$api-documentation-quality-in-rust`.
Use `$unsafe-code-in-rust` for Rust `SAFETY:` comments and caller-facing unsafe
contracts.

## Author procedure

When documenting a code caveat:

1.  Classify it as irreducible complexity, a retained workaround, or deferred
    incomplete or defective work.
2.  Confirm whether the target is first-party code or part of a vendored fork,
    and preserve any mechanically required comment syntax.
3.  Make the code itself as clear as the task reasonably allows.
4.  Write the comment with the category-specific rationale, evidence, and future
    condition described above.
5.  Verify that every factual claim and citation applies to the current code.
6.  Re-read the comment without the implementation and remove narration that
    adds no maintainer knowledge.
7.  Review only other comments directly implicated by the same task.

## Reviewer procedure

Review the same task-local facts, scope boundary, and mechanical requirements an
author should have checked.

Report a finding when an affected comment:

- narrates code without preserving a non-obvious reason, invariant, constraint,
  or future condition;
- explains complexity that a clearer in-scope implementation can remove;
- makes a stale, false, unsupported, or misleading claim;
- omits an applicable primary citation for an externally defined complex
  algorithm;
- omits the required `WORKAROUND:` prefix from a retained workaround;
- labels a workaround without identifying the constraint that requires it;
- lacks the controlling ticket or authoritative reference when one exists;
- gives no concrete trigger and removal condition when no workaround reference
  exists;
- leaves a TODO or FIXME without identifying the incomplete or defective
  behavior;
- uses `TODO:` for a known defect or `FIXME:` for merely incomplete work;
- omits the blocker for deferred work outside the explicit-stub exception; or
- uses a marker in place of completing unblocked work that belongs in the task.

Do not report unrelated narrative comments merely because the skill has loaded.
Do not demand explanatory comments for code that is already clear, and do not
apply this skill's prose rules to documentation comments, safety arguments, or
mechanically significant comments.
