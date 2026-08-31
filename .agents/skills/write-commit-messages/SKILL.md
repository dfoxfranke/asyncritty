---
name: write-commit-messages
description: Draft, revise, and audit Git commit messages and the structure of a Git patch series. Use when preparing commits on a branch intended for human-submitted review, rewriting messages, reviewing staged changes, creating review-response fixups, or cleaning history after approval. Produces durable, self-contained commit history while keeping pull-request summaries human-authored.
---

# Write Commit Messages

Use this skill to write exact Git commit messages and to evaluate whether a
branch is organized into reviewable commits. Pull-request communication is out
of scope: a human authors and submits it separately.

## Governing model

Treat each commit as one durable implementation step: state what its patch
changes, the technical problem or invariant involved, and any non-obvious
reason for the implementation. Keep the message useful after forge discussion
is unavailable. Leave whole-change motivation, externally observable results,
issue references, benchmarks, and validation reports to the human-authored pull
request.

## Non-negotiable rules

### Never include forge issue or ticket references

Never put issue, ticket, or pull-request references in a commit message. This
includes:

- `#123`, `GH-123`, Jira-style IDs, or similar identifiers;
- `Closes`, `Fixes`, or `Resolves` directives aimed at forge issues;
- links to issues, pull requests, review comments, or project-board items;
- phrases such as “requested in issue 123” or “address review feedback.”

Include the necessary technical context instead of delegating meaning to a
forge artifact.

A project-standard reference to a *commit*, such as
`Fixes: <stable-hash> ("subject")`, is not an issue reference. It is permitted
only when the hash satisfies the stability rule below.

### Cite another commit by hash only when the hash is stable

A commit message may name another commit by hash only when that commit is
already on a protected branch and therefore will not be rewritten. Do not cite
hashes from:

- the current patch series;
- an unprotected topic branch;
- a contributor fork whose history may be rebased;
- any branch whose protection status is unknown.

If stability cannot be established, treat the hash as unstable. Describe the
change by name or quote its subject instead. When a stable hash is useful,
include enough of the referenced subject to make the relationship intelligible
without looking it up.

### Keep pull-request material out of commit messages

Do not include:

- issue or ticket metadata;
- benchmark results or benchmark transcripts;
- commands run, manual-testing reports, or test-environment matrices;
- rollout instructions or reviewer instructions;
- product, customer, organizational, or other human motivations for adding a
  feature or intentionally changing non-defective behavior;
- screenshots, review chronology, or discussion summaries.

A commit may describe test code that the patch adds and the behavior that code
covers. It must not turn the commit body into a report of tests the contributor
ran.

A commit may explain an algorithmic property or technical constraint, but not
measured performance figures.

### Record technical reasoning, not development chronology

Explain root causes, invariants, constraints, tradeoffs, and non-obvious design
choices. Do not narrate development chronology with messages such as:

- “fix typo” when the typo was introduced earlier in the same series;
- “oops” or “fix previous commit”;
- “address review comments”;
- “try another approach”;
- “make tests pass” without explaining the code-level defect.

Before initial review, fold such corrections into the commit that introduced
them. During review, preserve reviewed history and append fixup commits as
described below.

### Do not invent intent or rationale

Ground every statement in the diff, surrounding code, tests, repository
documentation, or explicit human-provided context. Do not manufacture a root
cause, compatibility claim, performance claim, or design rationale merely to
make the message sound complete.

If no non-obvious rationale is supported, omit the body rather than inventing
one.

## Patch-set lifecycle

The correct history shape depends on whether review has started.

### Before a human opens a pull request: prepare the branch

Prepare the branch intended to become a pull request as a clean, LKML-like
ordered patch series. The human will author and submit the pull request
separately. The branch history should express the logical structure of the
solution, not the literal chronology of development.

For each proposed commit, be able to explain why a reviewer benefits from seeing
it separately. Good reasons include:

- isolating a mechanical or preparatory refactor from a behavioral change;
- introducing an invariant or representation before using it;
- separating an independently useful cleanup from the feature it enables;
- keeping an unrelated formatting or generated-file update out of a substantive
  diff;
- separating human-written scaffolding from tool-assisted implementation when
  that division is real and reviewable.

Do not split merely to reduce line count. If two changes require the same
explanation, cannot be reviewed independently, or leave an artificial boundary,
they probably belong in one commit.

Order commits so that prerequisites precede their consumers. Every commit must
keep the build working and must not introduce a regression relative to its
parent. A commit may add a regression test that fails because it exposes a bug
already present on the protected base branch; that expected failure is evidence
of the existing bug, not a regression introduced by the patch. The test commit
must itself build, and the test must fail for the intended reason.

When a bug-fix series includes a regression test, put the regression-test commit
before the fix. Reviewers should be able to see the test fail at that commit and
pass once the fix is applied. Apart from this deliberate test-only exception,
no commit should introduce a new build or test failure that a later commit must
repair. The complete series must build and pass its tests.

A commit need not be a complete standalone product feature, but it should be an
internally coherent unit of review and durable history.

Fold all corrections to an unreviewed patch back into that patch. The initial
series must not contain cleanup commits for mistakes introduced by earlier
commits in the same series. A standalone correction to code already present on
the protected base branch is different and may be a valid commit of its own.

### While code review is in progress: append, do not rewrite

Once review begins, do not rebase, amend, squash, reorder, or otherwise rewrite
the commits reviewers have already seen. Append patches that respond to review
feedback to the tip of the branch so reviewers can see exactly what changed
relative to the reviewed state.

A correction intended to be folded into an earlier commit may use a
`fixup! <target subject>` commit. A genuinely new logical step may use a normal
commit message. Do not disguise substantive new behavior as a fixup.

During this phase, preserving review continuity takes priority over presenting a
perfect final history. Do not perform history-rewriting operations unless the
review process has ended and approval has been granted.

### After approval: restore the clean series without changing the tree

After code review is approved, rebase or autosquash the appended review-response
commits into a clean final patch set. Rewrite final commit messages as needed so
each resulting commit follows this skill.

This cleanup must not change the approved code. Record the approved `HEAD` and
its tree before rewriting:

```sh
approved_head=$(git rev-parse HEAD)
approved_tree=$(git rev-parse 'HEAD^{tree}')
```

After the rebase, verify that the tree at the new `HEAD` is byte-for-byte
identical:

```sh
test "$approved_tree" = "$(git rev-parse 'HEAD^{tree}')"
```

An equivalent empty diff against `$approved_head` may be used as an additional
check. Commit IDs and commit metadata will change during a rebase; the final
Git tree must not. If the tree differs, the operation introduced a content
change rather than merely cleaning history, and that change requires review.

Do not leave `fixup!`, `squash!`, “address review,” “fix typo,” or similar
review-process commits in the final series.

This skill may recommend a reorganization, but it must not rewrite Git history
or create commits unless the human explicitly requests that operation.

## Workflow

### 1. Read repository-specific rules

Before drafting, inspect the repository's contribution guide, commit lint
configuration, existing trailers, and recent protected-branch history. Follow a
clear local convention for component prefixes, capitalization, line wrapping,
and trailer order. Do not copy weak recent messages merely because they exist.
For a series, identify the protected base rather than inferring it solely from a
branch name.

### 2. Establish the review phase

Determine whether the branch is:

- being prepared for a human to submit for initial review;
- currently under review; or
- approved and ready for final history cleanup.

Apply the corresponding lifecycle rules. If the phase is uncertain, do not
silently recommend history rewriting.

### 3. Inspect the full patch, not just filenames

Read the actual diff and relevant surrounding code. Establish the before and
after states, the technical limitation or invariant, the implementation choice,
and any supported non-obvious constraints. For a series, inspect both each
commit and the cumulative diff; a message for the final tree may be wrong for
the intermediate commit that carries it.

### 4. Check the commit boundaries

Check that each diff is one logical step without unrelated changes and that its
boundary aids review. When tooling permits, build and run relevant tests at
each commit rather than inferring intermediate integrity from the final `HEAD`.
Only a regression-test commit exposing a pre-existing bug may introduce a test
failure, and it must fail for the intended reason.

When boundaries are wrong, report the structural problem before polishing the
messages. Good prose cannot make a poorly organized patch set coherent.

### 5. Separate commit facts from pull-request facts

Put exact code changes, root causes, invariants, technical choices, relevant
test code, and required trailers in commit messages. Omit issue references,
human or product motivation, benchmarks, validation reports, compatibility,
rollout, and reviewer notes. The skill may name omitted categories but must not
draft their pull-request prose.

### 6. Draft the subject

State the specific action, name the component when useful, and follow the
repository's style. Prefer imperative mood, roughly 50–60 characters, no
trailing period, and no vague placeholders such as “update,” “misc,” or
“address feedback.” Never include an issue reference or unstable hash.

### 7. Draft the body only when it adds durable value

After a blank line, explain only what the diff does not communicate reliably:
the prior limitation or invariant, root cause, implementation choice, important
tradeoff, or consequence to preserve. Do not restate the subject or diff.

When referring to another patch in the same series, prefer a conceptual
relationship such as “Now that the parser returns normalized predicates…” or
name the other patch by subject. Avoid patch numbers when reordering would make
them stale, and never use a rewritable hash.

Wrap according to repository convention, commonly near 72 columns.

### 8. Add required trailers

Preserve valid project-required trailers and keep them after a blank line at the
end of the message.

Contributions substantially assisted by AI or another specialized tool require
an `Assisted-by` trailer. Use:

- `harness:vendor/model` for AI systems, using the established `vendor/model`
  identity when available;
- `tool:version` for other specialized tools.

Multiple tools may appear on one trailer line, for example:

```text
Assisted-by: codex-cli:openai/gpt-5.5 cargo-fix:1.96
```

Include AI models, static analyzers, fuzzers, `cargo fix`, and similarly
substantive assistance. Do not include routine Rust tooling such as `rustfmt`,
`cargo clippy`, or `rust-analyzer` merely because it was used normally.

Generating only the English subject line with AI does not by itself require
disclosure. Broader drafting or substantive assistance may. Apply the project's
substance threshold to each commit rather than treating tool use as microscopic
contagion.

Never guess a tool identity or version. Use information provided by the runtime,
repository, or human. If a required identity is unknown, flag that the message
cannot be finalized until it is supplied; do not emit a fake identity or an
unresolved placeholder as a final trailer.

### 9. Audit the result

Verify the message against the rules above. For a series, also verify its order,
intermediate integrity, review-phase handling, and—after approval—the approved
tree identity.

## Message patterns

### Defect fix

State the implementation failure and the invariant restored. Do not rely on an
issue report to explain the bug.

```text
parser: handle empty predicate lists

An explicitly empty filter reaches build_conjunction with no child
predicates. The builder assumes at least one child and indexes the first
element, so the empty form panics after normalization.

Return the identity predicate for an empty list. Keeping the empty case in
the builder preserves one invariant for every caller and avoids duplicated
special cases.
```

### Trivial standalone correction

A body is unnecessary when the subject completely explains a small correction
to code already present on the protected base branch.

```text
docs: correct the Environment heading
```

If that misspelling was introduced by an earlier unreviewed commit in the same
series, amend that commit instead of creating this one.

## Output contract

When asked for one commit message, return the exact message in a plain-text code
block. Add concise diagnostic notes outside the block only when there is a
structural problem, missing required identity, or PR-only material that was
omitted.

When asked for a series, present commits in order. For each commit, identify the
commit or proposed position and provide the exact message. Report any required
split, fold, reorder, fixup, or post-approval cleanup separately from the message
text.

When asked to audit existing messages, identify each violated rule and provide a
corrected message grounded in the corresponding diff.

Never generate a pull-request summary. When PR-only facts are encountered, name
only the omitted categories so the human can write the summary independently.
