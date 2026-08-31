---
name: "test-quality"
description: >-
  Use this skill when adding, revising, reviewing, or comprehensively
  assessing tests or test suites; determining whether intended contracts are
  adequately covered; choosing unit, property, integration, or acceptance
  tests; refactoring for testability; or reporting residual verification
  issues and assessment limitations.
---

# Test quality

The goal is to verify the intended contract as thoroughly as practical without
turning incidental behavior into promises or burdening the test suite with
redundant, brittle, slow, or non-portable checks.

Use `$api-documentation-quality` and, for Rust,
`$api-documentation-quality-in-rust` when reviewing or changing API
documentation. Tests and the project's records of intent must agree at each
audience boundary. Every assertion needs a supported preservation basis: an
applicable public or user contract, or an evidence-backed non-public invariant
that maintainers intend to preserve.

## Apply repository scope and resolve task boundaries

For first-party code in this repository, apply this skill's rules directly and
authoritatively within the task's assessment and change boundaries. Do not
weaken them merely because nearby tests follow a different practice.

If the target is inside a vendored fork, mirror, or downstream-maintained copy
of third-party source, use `$work-in-vendored-forks` first and apply the
upstream conventions it resolves within that fork. First-party code that only
calls, wraps, adapts, tests against, or otherwise interacts with a third-party
dependency is not fork maintenance; apply this skill normally, including all
dependency-interfacing rules below.

Derive two independent boundaries from the user's task:

- The **assessment boundary** identifies the contracts, behavior, evidence,
  configurations, and regressions to evaluate at the requested breadth.
- The **change boundary** identifies the files and behavior the task authorizes
  modifying. Treat it as empty for review- or report-only work.

Treat explicit inclusions, exclusions, and breadth qualifiers as controlling.
Unless the prompt restricts changes, treat a task to add or improve tests as
authorizing affected tests and ordinary testability refactors to the code under
test, including maintainer-facing invariant records but not its public contract.
Inspect immediate collaborators as needed to judge an in-boundary proposition;
inspection alone does not authorize modifying them. Record any later boundary
refinement, never expand transitively into unrelated subsystems, and declare
both boundaries and any unassessed or inaccessible surfaces in the final report.

Treat configured test frameworks, manifests, required runners, compilers, and
build targets as hard constraints. Inspect existing tests only as needed to
establish contract evidence, understand those interfaces, locate reusable
fixtures, or avoid duplicating compatible infrastructure. Existing tests can
serve as executable specifications only under the preservation-evidence rules
below; they cannot waive this skill's rules for first-party code.

For a scoped contribution, do not turn the task into a repository-wide audit,
introduce documentation merely to establish a new culture, or expand the change
boundary for an unrelated gap. Record a concrete incidental gap without
recursively auditing its surrounding subsystem.

## Establish the effective contract and preservation basis

Derive the effective contract and supported non-public invariants from all
applicable evidence, including:

- explicit task requirements, acceptance criteria, and maintainer direction;
- declarations, types, and inherited or enclosing contracts;
- API, user, protocol, schema, and file-format documentation;
- applicable standards and explicit compatibility commitments;
- maintained maintainer-facing invariants and deliberate executable
  specifications; and
- maintained callers, regression history, examples, changelogs, coherent sibling
  behavior, and deliberate design structure.

An explicit scoped requirement or authoritative source can establish the weakest
proposition it entails unless a higher-precedence source conflicts. Otherwise
require multiple independent indicators of preservation intent. Treat focused
existing tests as evidence when the project uses tests as executable
specifications, but not automatically as public API guarantees.

Current implementation behavior alone is insufficient. So are one isolated or
opaque test or snapshot, pass-before or pass-after behavior, and the value or
ease of testing the proposition. Never let a new test bootstrap its own
authority. Interpret prose-documentation silence alongside the repository's
maintained specification artifacts: silence in non-exhaustive documentation does
not prove behavior intentionally variable, but undocumented behavior does not
thereby become a caller-facing guarantee.

Resolve conflicts using project-defined precedence. Without one, test only
unambiguous common guarantees and report a specification conflict. Never assert
incidental behavior merely because the current implementation exhibits it. In
the rest of this skill, a **contract proposition** includes a supported
non-public invariant as well as a public or user-facing guarantee.

### Record supported private invariants

When a useful oracle is more precise than the public contract, first prefer the
weakest semantic relation that can expose the fault. If maintainers genuinely
need to preserve an exact private detail, record the weakest sufficient
invariant using this carrier order:

1.  Use owner-adjacent non-public documentation.
2.  If documentation is disproportionate to the invariant, use a meaningful
    name, constant, type, assertion, or concise ordinary source comment beside
    the owning code.
3.  Only when owner-side recording would be disproportionate or outside the
    change boundary, use a precise test declaration, fixture name, or test-local
    comment that states the proposition and rationale.

Keep the record out of generated public API documentation. A test-side record is
a constrained fallback, not permission to preserve implementation output: the
invariant still needs independent support under the evidence rules above. Adding
or revising a suitable non-public record within the change boundary is part of
the default permission to refactor for testability and does not trigger
documentation of neighboring items.

### Handle suspected documentation gaps

When a useful test proposition is stronger or more precise than the currently
documented contract, decide independently whether callers should receive that
stronger guarantee and whether maintainers should preserve it privately. The
public contract may be appropriate even when the second answer is yes.

Before treating existing documentation as accidentally weak, require the
proposition to be objective, stable, and relevant at the affected boundary. Also
require either:

- an authoritative requirement, enclosing contract, or established invariant
  that entails it; or
- multiple independent indicators that maintainers or first-party consumers
  intend to preserve it.

For a suspected missing public guarantee, require caller-facing preservation
evidence independent of the implementation under test; repeated observations of
implementation shape remain insufficient. Do not strengthen a contract when
another applicable source intentionally leaves the behavior unspecified or
permits it to vary.

Classify documentation by the claim's audience, not merely the owning item's
visibility:

- **Non-public contract or invariant:** Record and test the weakest supported
  invariant using the carrier order above. Establishing one may constrain future
  implementations without implying earlier documentation was mistaken.
- **Public or user-facing contract:** Revise it only when required by the task
  or its acceptance criteria. Apply `$api-documentation-quality` and any
  applicable language profile, add only the weakest supported caller-facing
  guarantee, and test it.
- **Public documentation outside the task:** Leave it unchanged. Test only a
  supported preservation basis, and report an evidence-backed missing public
  guarantee when it concerns an assessed proposition, even when other evidence
  supports the regression test.

A public contract may be appropriately weaker than a separately recorded private
invariant. Preserving the latter exposes nothing to callers and does not imply
deficient public documentation. Conversely, do not disguise a suspected missing
public guarantee as a private invariant merely to authorize a stronger test.

Do not report a repository's general lack of contract documentation as a gap.
Report only a concrete in-boundary ambiguity, conflict, or evidence-backed
missing public guarantee; a documentation gap need not block testing to merit
this report.

If an evidence-backed candidate remains insufficiently supported or
authoritative sources conflict, preserve the existing contract, test only its
unambiguous guarantees, and report the specification question. Discard
unsupported imagined detail rather than reporting it. Treat behavior that
contradicts the effective written contract as an implementation defect, not a
reason to weaken the oracle or relabel it a documentation gap.

## Assess contract coverage

Treat thoroughness as coverage of contract obligations, not source lines,
branches, test counts, or arbitrary input samples.

For every independently falsifiable contract proposition in the assessment
boundary, identify the meaningful behavioral classes that could violate it.
Consider:

- successful outcomes and relationships among inputs and results;
- preconditions, boundaries, special values, and partitions with different
  semantics;
- documented errors, recovery, and side effects;
- state transitions, ordering, repetition, and interaction between dimensions;
- supported features, platforms, and configurations; and
- known regressions and dependency or environment assumptions.

Map each proposition and behavioral class to existing evidence. A stronger
property test may subsume examples; the type system or static analysis may make
a runtime test redundant; an integration or acceptance test may be the
appropriate evidence.

Challenge each apparent gap with this question:

> What is the smallest plausible contract violation that could still compile and
> pass the existing verification?

Treat the case as distinct only when the answer identifies a different promise,
outcome, boundary, state interaction, configuration, assumption, or plausible
defect mechanism. Another value governed by the same reasoning is only another
sample. Do not construct a Cartesian product of independent dimensions.

Use line and branch coverage, mutation testing, fuzzing, and similar tools to
discover candidates, never as proof of either completeness or a finding. An
uncovered line may represent no independent promise, and a surviving mutant
matters only when it produces a contract-observable violation.

## Decide which verification issues to address

Treat both genuine coverage gaps and defects in existing tests as **verification
issues**. Maintain:

- a **verification ledger** containing each contract proposition with only its
  distinct meaningful behavioral classes or fault mechanisms, plus every
  identified test-quality defect;
- an **assessment-limitations ledger** containing inaccessible surfaces,
  unresolved evidence-backed documentation questions, specification conflicts,
  and other conditions that prevent an assessment; and
- a **work queue** containing authorized verification work selected below.

Give each verification-ledger entry one disposition. **Covered** means current
evidence adequately verifies it; **subsumed** means stronger evidence or an
encompassing issue accounts for it; **queued** means its authorized remedy is
pending; **addressed** means the remedy passed verification; and **residual**
means a genuine gap or defect remains.

Do not create a coverage-gap entry without a supported preservation basis and
distinct escaping fault. Put an evidence-backed clarification question or
unassessed surface in the limitations ledger; discard imagined detail without
such evidence. Record an existing-test defect when it meets the reviewer
criteria.

Treat a verification issue as **actionable** when a reliable, repeatable remedy
exists and its expected protection or quality benefit justifies the least-
burdensome remedy after considering portability, default-suite speed,
reliability, dependency and build burden, maintenance coupling, and API or
refactoring risk.

Put an actionable issue in the work queue only when its complete remedy and
prerequisites lie within the change boundary and are unblocked. Otherwise retain
it as an actionable residual and record the boundary or blocker.

Low implementation cost establishes low burden, not sufficient value. Queue
compelling and material issues when a reliable remedy is proportionate. A
limited issue remains residual by default even when easy to fix; queue it only
when its concrete incremental benefit clearly outweighs its ongoing suite and
maintenance cost. When value and burden are genuinely close, favor remedying
compelling or material issues, not limited ones. Mere implementation effort, the
issue's age, a high coverage percentage, or confidence that the code is probably
correct do not justify deferring a compelling or material issue.

Assess verification value qualitatively:

- **Compelling:** An explicit task acceptance criterion remains unsatisfied; a
  current defect or credible escaping fault threatens safety, security,
  authorization, money, data integrity, compatibility, concurrency, or another
  high-consequence behavior; or a test defect invalidates material verification
  or prevents a supported test or CI environment from running reliably.
- **Material:** A current defect or distinct plausible fault has meaningful
  caller, verification, CI, or maintenance impact, and existing verification or
  safeguards do not adequately mitigate it.
- **Limited:** The fault is genuine but remote or low-impact, or incomplete
  indirect evidence makes its incremental verification value small.
- **None:** A proposed coverage candidate concerns incidental or unsupported
  behavior, is redundant, or lacks a distinct fault model; or an alleged test
  defect violates no applicable quality rule. It is not a residual issue.

Assess portability, speed, reliability, and maintenance burdens separately and
support each judgment with concrete facts. Treat flakiness primarily as a
reliability burden, brittle or circular oracles as reliability and maintenance
burdens where applicable, and undeclared system facilities or special services
as portability burdens. Use measurements when readily available, but do not
invent probabilities or collapse the judgments into a numeric score.

## Work to convergence without approval checkpoints

Process the work queue in descending expected value, favoring changes that close
several obligations without coupling tests to implementation details:

1.  Select the strongest actionable issue.
2.  Add, repair, or remove tests, acceptance procedures, or testability code as
    needed to remedy it.
3.  Run targeted verification and then the applicable broader suite.
4.  Update the verification ledger for behavior affected by the change.
5.  Continue until the work queue is empty.
6.  Reconcile contract-to-test and test-to-contract mappings, then repeat the
    counterexample and test-quality review across the applicable dimensions.
7.  Reopen the queue whenever reconciliation exposes an actionable issue.

Treat a convergence pass as one review of every current verification-ledger
entry. Add an entry only for a distinct in-boundary proposition, behavioral
class, fault mechanism, or test-quality defect; restatements and extra samples
do not reopen the queue. After the last change, stop when one complete pass adds
no actionable entry. When exhaustive inventory is impossible, record the exact
limitation and finish without claiming conformance.

Do not interrupt the user for prioritization judgments, isolated contract
ambiguities, or permission to leave a residual issue. Continue all independent
work and collect unresolved matters for the final report. If new authority is
indispensable and no other meaningful work within the change boundary can
proceed safely, batch all currently known authority questions into one request.
Do not ask about choices that can safely remain residual. Honor repository-,
environment-, and platform-required approvals; this instruction does not bypass
them.

Finish only when:

- the assessment boundary has been inventoried or every limitation recorded;
- every verification-ledger entry has a disposition;
- no actionable issue whose complete remedy is authorized and unblocked remains;
- changed verification passes on applicable project-declared targets available
  in the current workspace or through accessible CI evidence without new
  authority; and
- a final reconciliation exposes no new actionable counterexample.

An authorized, unblocked actionable issue demonstrates that the work has not
converged. Record an actionable issue with an out-of-boundary prerequisite or
external blocker as a residual and state the reason. Record unavailable
environments and configurations as assessment limitations rather than treating
every theoretically constructible combination as attainable.

If a new test exposes an implementation defect whose fix lies outside the change
boundary, do not leave the default suite failing. Preserve reproducible
evidence, classify the issue as blocked by an out-of-boundary prerequisite, and
report it as a compelling residual. Put an executable reproducer behind an
explicitly opt-in target when practical; never commit an expected-failing test
to the default suite.

## Choose robust test techniques

### Use properties for universal contracts

When a contract quantifies over an argument domain, prefer a property test if it
provides materially stronger evidence. Ensure its generator spans the promised
domain and its oracle does not reproduce the implementation.

Use finite examples when they exhaust the meaningful behavioral classes or a
property test would add no distinct fault-detection value. Do not report the
absence of a property test when a short, obviously sufficient example set covers
the proposition. Preserve explicit boundary or regression examples when
generation does not guarantee them. Do not imitate property coverage with a long
hand-picked list: either name the distinct semantic partition or fault mechanism
exercised by each example, use a property test when its added value is
proportionate, or keep the smallest representative set.

### Make justified assumptions explicit

Tests must not turn undocumented third-party accidents into first-party
guarantees. When a test must rely on undocumented third-party behavior that is
currently true and would reasonably be treated upstream as semver-relevant,
record the assumption in a comment beside the test.

Allow reasonable assumptions about the local test environment. Comment on a
non-obvious assumption, such as requiring a temporary path to be valid Unicode,
but omit foundational assumptions such as functioning CPU and memory.

### Keep test doubles at the contract boundary

Use a test double only to create a contractual scenario when direct integration
would be unsuitable. Assert the promised observable result, not how code called,
ordered, or disposed of the double unless that interaction has a supported
preservation basis. Test first-party adaptation rather than the dependency's own
contract.

### Remove tests for removed behavior

Remove all test-suite vestiges whose only purpose was to exercise a removed
feature, including tests, fixtures, helpers, snapshots, and configuration. Do
not convert them into rejection tests unless rejecting the old behavior is
itself an intended compatibility guarantee.

### Refactor and depend deliberately

Treat refactoring code within the change boundary for testability as
presumptively authorized when adding or improving tests. Never make a
semver-breaking public API change under that permission.

Allow tests to use heavier dependencies than production code when they provide a
stable, independent oracle. Prefer a portable off-the-shelf test dependency to
bespoke infrastructure that must be maintained in-tree. Absent contrary task
direction, do not introduce heavyweight custom test infrastructure or a
dependency that makes previously supported, reasonable build environments unable
to build or run the default suite.

### Keep default tests portable and isolated

Ensure tests run by default can succeed in every supported, reasonable developer
and CI environment, including environments without network access. Require a
host library, tool, service, or other facility only when the package manifest or
developer documentation explicitly declares it as a development prerequisite.
Treat manually installed host facilities as a portability cost even when they
are off the shelf; a portable, manifest-resolved dev dependency may be cheap.
Put valuable checks requiring credentials, special configuration, user
interaction, or other non-standard facilities behind an opt-in test target and
document how to run them.

Where a framework distinguishes unit and integration tests, keep unit tests
observationally pure: avoid console output, filesystem access, and sockets. Make
mutation of process-global state safe under parallel and repeated execution, and
join every spawned thread before the test completes. Move tests that require I/O
into the framework's integration-test or other non-unit layer.

Apply the same purity rule to executed documentation examples. Prefer a useful
example when purity conflicts with pedagogy, and mark it non-executing with
`no_run` or the local equivalent when necessary.

On success and failure, undo filesystem changes made by a test before it
returns, restoring or removing files and directories as appropriate. Provide one
suite-wide mechanism to retain the changed state for debugging rather than
multiplying per-test knobs.

### Use repeatable acceptance tests for human judgments

Use acceptance tests for correctness properties whose oracle requires human
judgment. Give the tester detailed, repeatable setup, execution, and inspection
instructions. The qualitative criterion may remain subjective, but the procedure
for reaching it must not be vague.

## Author procedure

When adding or improving tests:

1.  Classify the target as first-party code or vendored-fork code, apply
    `$work-in-vendored-forks` when required, and resolve the boundaries,
    effective contract, and supported preservation bases.
2.  Inventory independently falsifiable propositions and meaningful behavioral
    classes before choosing easy test cases.
3.  Map existing tests and stronger evidence to that inventory.
4.  Resolve suspected documentation gaps without inventing or laundering
    guarantees.
5.  Identify a plausible escaping fault for every proposed addition.
6.  Deduplicate samples and independent dimensions.
7.  Select the least-burdensome reliable verification layer.
8.  Refactor for testability and clarify supported private invariants when
    appropriate, without breaking public APIs.
9.  Remedy actionable verification issues in descending expected value and
    verify each change proportionately.
10. Reconcile both directions: every contract obligation should have evidence or
    an explicit residual disposition, every actionable obligation should have
    evidence unless its remedy is outside the change boundary or blocked, and
    every assertion should have a supported preservation basis.
11. Repeat the counterexample challenge until it produces no new actionable
    fault class or test-quality defect.
12. Report every residual verification issue and assessment limitation.

## Reviewer procedure

Perform the same target classification, contract inventory, evidence mapping,
and counterexample challenge an author should have performed, including use of
`$work-in-vendored-forks` for a vendored-fork target.

Report a finding when tests:

- assert incidental behavior without a supported preservation basis;
- omit a distinct actionable contract obligation within the assessment boundary;
- use samples that miss a meaningful boundary or behavioral class and thereby
  leave an actionable gap;
- use a finite sample where a practical property test would materially improve
  coverage of a universal proposition;
- use a circular or unstable oracle;
- preserve vestiges of removed behavior without an intended compatibility
  guarantee;
- make an undocumented and non-obvious third-party or environment assumption;
- introduce avoidable flakiness, default-suite environmental requirements, or
  disproportionate runtime and maintenance burden;
- leave an expected-failing test in the default suite;
- perform console, filesystem, or socket I/O in a unit test or executed
  documentation example;
- leave filesystem artifacts without a default cleanup path;
- use an irrepeatable human acceptance procedure; or
- silently omit a genuine residual or assessment limitation from a purportedly
  comprehensive report.

Do not report a finding merely because another input could be sampled, a line or
branch is uncovered, a mutant survives without violating the contract, a
property test is absent when examples exhaust the meaningful classes, a heavier
test-only dependency is used responsibly, or a supported private invariant is
stronger than the appropriate public contract.

## Report residual issues comprehensively

Report every genuine residual within the declared assessment boundary, not only
recommended follow-up work. Include concrete incidental issues encountered
outside the change boundary, but distinguish them from a systematic assessment
of that surrounding code.

Order residuals first by verification value: compelling, material, then limited.
Within each tier, put actionable issues blocked by external conditions or
excluded from the change boundary first. Then prefer one issue over another when
it has no greater portability, speed, reliability, or maintenance burden on
every axis, with at least one strict advantage. Leave incomparable gaps as peers
rather than inventing a precise total order.

Tag environment, hardware, credentials, human judgment, specification conflict,
inaccessibility, and similar reasons without using those reasons as priority
tiers. Report assessment limitations separately when insufficient evidence
prevents assigning a reliable verification-value tier.

For each verification residual, state:

- the contract proposition or test-quality rule and its source;
- a concrete escaping fault or existing adverse effect;
- existing evidence and why it is insufficient;
- the best proposed remedy and verification;
- expected verification value or quality benefit;
- portability, speed, reliability, and maintenance effects;
- why the issue remains and its likely consequence; and
- the condition under which it should be revisited.

For each assessment limitation, state the affected surface or conflicting
sources, why no determination was possible, the consequence, and the condition
that would unblock or revisit it. For a suspected documentation gap, also state
the current effective contract, the stronger candidate and its evidence, what
tests can and cannot establish, and the documentation change that would be
needed.

Group equivalent cases only when they share a contract or quality-rule source,
fault class, and remedy, and identify every affected item in the group. Do not
write source TODOs for residual issues unless requested.

Treat an assessment limitation as material when it prevents determining whether
an in-boundary proposition has adequate evidence or whether the tests satisfy an
applicable quality rule. Choose the conclusion in this order:

1.  **Standard not met within the assessment boundary:** Use this when one or
    more actionable verification issues remain because their remedies or
    prerequisites are outside the change boundary or externally blocked.
2.  **Work converged, but conformance could not be established:** Use this when
    no actionable residual is known but a material specification conflict,
    inaccessible surface, or other assessment limitation prevents a conclusion.
3.  **Standard met within the assessment boundary:** Use this when no actionable
    verification issue remains and no material assessment limitation prevents
    the conclusion; report all non-actionable residuals.

If no residual verification issue was identified, say so within the declared
boundary and evidence examined; do not claim that completeness has been proven.
