---
name: "work-in-vendored-forks"
description: >-
  Determine and apply upstream project conventions when adding, changing,
  reviewing, or testing code inside a repository-contained vendored fork,
  mirror, or downstream copy of third-party code. Use this skill before other
  repository skills when the target is maintained as a downstream fork; do
  not use it merely because first-party code calls, wraps, tests against,
  or depends on third-party software.
---

# Work in vendored forks

Preserve upstream compatibility and keep downstream divergence intentional when
maintaining third-party source inside the repository. Resolve only the
conventions relevant to the current task, then return them to the calling
repository skill.

## Confirm the fork and its boundary

Establish that the target is source maintained as a downstream fork, not merely
first-party code that interfaces with a dependency. Look for repository
instructions, provenance or import files, license and readme notices, package or
submodule metadata, patch queues, update scripts, recorded upstream URLs and
revisions, and directory-level ownership markers.

If the code only calls, wraps, implements an interface from, tests against, or
otherwise depends on third-party software, stop applying this skill. Return to
the calling domain skill and keep its dependency-interfacing guidance in force.

Within a confirmed fork, distinguish:

- upstream-owned source that should retain upstream conventions;
- deliberate downstream patches to that source;
- host-owned adapters, build glue, tests, or packaging beside the fork; and
- generated files that must be changed through their generator.

Apply upstream conventions to upstream-owned source and downstream patches
unless the precedence below establishes an intentional fork-specific divergence
for the affected code. Apply the repository skills' first-party conventions to
host-owned glue. Follow the generator workflow for generated files. Treat a task
spanning more than one category as separate convention scopes.

## Establish the upstream baseline

Identify the upstream origin and the revision, release, or import snapshot on
which the fork is based. Prefer evidence recorded in the repository over path
names or assumptions. Use conventions applicable to that baseline, not unrelated
conventions from current upstream HEAD.

Inspect preserved upstream instructions, contributor documentation, style
guides, configuration, and nearby source at the recorded baseline. If the exact
baseline is unavailable, use the closest version whose relationship to the fork
can be established and state the limitation. Do not invent provenance or widen
the task merely to reconstruct the entire fork history.

## Resolve the applicable convention

Use this precedence order for each task-relevant choice:

1.  Follow the user's task instructions and host-repository instructions
    explicitly scoped to the vendored subtree.
2.  Follow hard compiler, formatter, linter, generator, build, packaging, and
    host-integration constraints that affect the maintained fork.
3.  Follow a coherent fork-specific convention established by several
    intentional downstream patches. Do not treat unrelated host style,
    mechanical import changes, or incidental drift as such a convention.
4.  Follow explicit upstream instructions applicable to the recorded baseline.
5.  Inspect several analogous upstream items of the same component, kind,
    visibility, and output path. Prefer maintained examples close to the fork's
    baseline.
6.  If upstream evidence is absent or genuinely mixed, use the authoritative
    default from the repository skill that called this one.

Do not let general host-repository style override a contrary upstream convention
inside upstream-owned source. An explicit downstream instruction uses the first
tier; a coherent pattern of intentional patches uses the third. Never infer
fork-specific divergence from an isolated edit or omission.

Stop after resolving choices that affect the current task. Do not turn a scoped
change into an audit of the vendored subtree or upstream project.

## Return a convention map

Give the calling skill a compact result containing:

- whether the target is part of a vendored fork;
- the upstream-owned, host-owned, and generated boundaries that affect the task;
- the upstream origin and baseline, with the evidence used;
- hard host integration constraints;
- each resolved domain convention and its controlling evidence; and
- any material uncertainty that remains.

When fork status or provenance remains uncertain after reasonable inspection,
state what could and could not be established. Use preserved subtree evidence
where it is coherent and otherwise fall back to the calling repository skill's
default. Do not silently treat current upstream behavior as the baseline.

## Resolve documentation conventions

Determine which public and private items upstream documents, which forms count
as documentation, and how its toolchain enforces coverage. For Rust, resolve
whether private items receive rustdoc and which item kinds the upstream lint
policy covers. Resolve whether tests carry prose documentation, whether a test
framework's proposition text already serves that role, and how upstream records
private invariants without turning them into public promises.

Preserve the calling documentation skill's requirements for truthful, supported
contracts. Upstream style can decide coverage and form; it cannot justify
redundant, invented, incidental, or false guarantees.

## Resolve caveat-comment conventions

Determine the upstream meanings, spelling, placement, and citation practice for
`TODO:`, `FIXME:`, `WORKAROUND:`, or equivalent markers. Inspect several
analogous comments before treating tags as distinct. Preserve mechanically
significant syntax required by compilers, generators, or other tools.

When upstream supplies no useful convention, use `TODO:` for incomplete work,
`FIXME:` for known defective behavior, and `WORKAROUND:` for an intentional
departure forced by a concrete constraint.

## Resolve Rust error and panic conventions

Determine whether upstream imposes a panic policy stricter than the calling
skill, how affected public errors preserve compatibility, and how its reporting
boundary traverses source chains. Inspect nearby analogous error types and the
call paths affected by the task, stopping at handlers or presenters.

Do not relocate rules about interacting with third-party APIs, implementing
third-party traits, or propagating third-party failures into this skill. Those
remain governed by the Rust error-handling and API-documentation skills.

## Resolve logging conventions

Resolve the emitting component's framework, explicit-event-name policy, semantic
field keys, and severity calibration independently. Inspect dependency and
feature configuration, subscriber or logger setup, and several analogous
emitters at the same application or library layer. Do not introduce a second
framework for an isolated event merely to follow a fallback preference.

## Resolve mathematical-comment conventions

Resolve formula notation separately for ordinary comments and generated
documentation. Determine the actual documentation generator, version,
extensions, delimiters, escaping, and wrapping rules. Use several analogous
formulas from the same upstream output path; do not infer renderer support from
generic Markdown support or one isolated formula.

Keep source-encoding classification and generator capability checks with the
math and Unicode source-safety skills. This skill resolves upstream choices, not
whether a tool can safely carry or render them.

## Resolve names and punctuation conventions

Apply explicit upstream style and encoding requirements first. Infer implicit
spelling, transliteration, or punctuation precedent only from the exact file for
person names and punctuation; do not search neighboring files merely to
manufacture an implicit convention. Preserve structured and machine-consumed
text exactly.

## Resolve test conventions

Determine upstream test organization, framework idioms, fixture placement,
executable-specification practice, private-invariant carriers, unit-test purity,
default-suite environment requirements, cleanup and retained-artifact
mechanisms, expected-failure policy, and residual-issue marker practice.

Upstream organization can override the calling skill's layout preferences. It
cannot authorize assertions without a supported preservation basis, circular
oracles, undeclared host requirements on the actual default suite, or silence
about material assessment limitations.

## Resolve Unicode source constraints

Assess both the upstream-supported source-processing path and the host
repository's actual import, patch, build, documentation, and generation paths.
Literal non-ASCII text is safe only when every supported path preserves and
decodes it consistently. Treat this as a technical constraint, not a style
choice that precedent can override.

## Resolve unsafe-code policy

Apply an upstream or subtree-specific prohibition or restriction when it is
stricter than the repository unsafe-code skill. Inspect compiler and lint
configuration as hard constraints. Do not treat permissive upstream practice,
widespread unsafe code, or missing restrictions as permission to weaken the
calling skill's eligibility rules.

No convention can waive Rust soundness, caller-facing `# Safety` contracts, or
implementation-side `SAFETY:` proof obligations.

## Preserve non-conventional requirements

Use upstream conventions only to resolve choices of coverage, organization,
representation, and style. Never let them override the user's authorized change
boundary, hard tool constraints, public compatibility, factual accuracy,
contract-evidence requirements, source-transport safety, or language soundness.
