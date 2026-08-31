---
name: "error-handling-in-rust"
description: >-
  Use this skill when adding, changing, or specifically reviewing Rust
  error handling, including API design, choosing between errors and panics,
  selecting structured or open-ended error types, writing panic-site messages,
  and preserving error source chains.
---

# Error handling in Rust

The goal is to make invalid states difficult to construct, represent recoverable
failures in forms callers can use, and reserve panics for whole-program bugs
without losing or duplicating diagnostic information.

## Apply repository scope

For first-party code in this repository, apply this skill's rules directly and
authoritatively within the task's change and review boundaries. Do not weaken
them merely because nearby code follows a different practice.

If the target is inside a vendored fork, mirror, or downstream-maintained copy
of third-party source, use `$work-in-vendored-forks` first and apply the
upstream conventions it resolves within that fork. First-party code that only
calls, wraps, implements an interface from, tests against, or otherwise
interacts with a third-party dependency is not fork maintenance; apply this
skill normally, including all dependency-interfacing rules below.

Treat compatibility as a requirement rather than a convention. Inspect the
affected error types and signatures, especially public ones. Trace the call
sites touched by the task and direct repository uses of every public variant,
field, source link, or rendered behavior the change would alter, stopping at the
first handler or presenter on each path. Preserve behavior on those paths when
callers match a variant, read a field, traverse a source chain, or rely on
rendered output.

## Core rules

### Make invalid states unrepresentable when the type earns its cost

Favor APIs that enforce correctness by construction. When a public function
requires an efficiently checkable input invariant, prefer a validated wrapper
type when the invariant is stable, domain-significant, reused across call sites,
or otherwise important enough to justify a distinct type.

Avoid one-off wrapper types whose ceremony exceeds their safety benefit.

### Reserve panics for whole-program bugs

Classify failures at the whole-program level. Represent a condition as an error
when it can arise from the filesystem, network, user, another process, or
another external input or system in a correct whole program after every
applicable caller obligation has been satisfied. Panic when reaching a condition
necessarily means that in-process code failed to uphold an obligation or
invariant. Except for an unavoidable panic originating in third-party code as
described below, do not panic for any other condition.

A function may therefore have a caller-facing panic condition: one that a
caller permitted by its effective visibility can trigger while satisfying the
item's other documented requirements. Document each such condition in
`# Panics`. Every caller must either establish that the condition cannot occur
or carry the corresponding condition into its own `# Panics` contract. Do not
invent a caller obligation merely to convert a recoverable failure into a panic.
This remains true when the offending value originally came from external input:
passing it without validation is the caller's bug. This skill does not require
per-call-site comments spelling out this argument.

Do not document a panic when every operation available to callers at the item's
effective visibility preserves a more-private invariant and the panic can occur
only because implementation code responsible for that invariant violates it.
That is an internal bug check rather than a caller-facing panic condition.

Treat every panic as an assertion failure, whether it is expressed with
`assert!`, `panic!`, `expect`, `unwrap`, indexing, or another panicking
operation.

### Preserve trait panic contracts

A trait implementation must not introduce a panic condition that the trait's
documented contract does not permit. This rule concerns behavior reachable while
the implementation and its dependencies satisfy their contracts; it does not
turn private-invariant bugs into documented panic conditions. If the codebase
controls the trait, either add the intended condition to the trait's contract or
make the implementation non-panicking.

An implementation of a third-party trait may introduce another panic condition
only when:

- the panic qualifies as an unavoidable third-party panic under the rule below;
  and
- there is clear evidence that the defining crate does not attempt to document
  all such panics.

An undocumented panic in an upstream implementation of the same trait is the
clearest evidence. A well-established pattern of undocumented panics across the
defining crate also suffices. The trait's silence or an isolated omission
elsewhere in the crate does not.

Document an accepted exception in a `# Panics` section on the implementation,
following `$api-documentation-quality-in-rust`.

### Allow only unavoidable third-party panics

An API may propagate a known panic from third-party code for an externally
originating condition when the dependency is used according to its contract and
there is no reasonable way to avoid the panic.

Before accepting such a panic, inspect the supported dependency versions,
features, and targets for a fallible API, a fallible composition of public APIs,
an existing compatible error channel, or another supported and reliable
alternative. An alternative is reasonable when it preserves the required
behavior and compatibility without disproportionate complexity or maintenance
cost. Do not treat a panic introduced by locally unwrapping a fallible result,
violating a dependency's preconditions, or speculating about an unknown
dependency bug as originating in the dependency.

`catch_unwind` does not avoid a panic. The panic still occurs and the panic hook
still runs even when unwinding is caught before it crosses the API boundary.

Document an accepted third-party panic as a possibility at the current API
boundary. Saying that the API `may panic` under the relevant condition is
sufficient; do not promise that it will panic.

### Use `Option` and `Result` deliberately at panic sites

Never use `Option::unwrap()`. When `None` would necessarily indicate a bug, use
`Option::expect()` and state why the value should be `Some`:

``` rust
let item = queue
    .pop_front()
    .expect("queue should contain the item enqueued above");
```

If `None` could instead represent a recoverable condition, handle it or convert
it to an error rather than calling either method.

Phrase every `expect` message as the reason the `Option` or `Result` should be
`Some` or `Ok`, following the standard library's [expect-as-precondition
guidance](https://doc.rust-lang.org/std/error/index.html#common-message-styles).
Do not merely restate the failure that occurred.

For `Result`, use `expect()` instead of `unwrap()` if and only if the message
can add informative context that the `Err` does not already carry. Otherwise use
`unwrap()` rather than adding a redundant message. Both forms are appropriate
only after establishing that an `Err` would necessarily indicate a bug.

Poisoned-mutex errors almost never benefit from an `expect` message. When mutex
poisoning represents a bug under these rules, normally write:

``` rust
let guard = mutex.lock().unwrap();
```

The `PoisonError` already identifies the failure; a message such as
`"mutex should not be poisoned"` merely repeats it.

### Expose only actionable structured library errors

Return structured error details from library APIs, but treat every exposed
detail as an API commitment. Choose the finest detail level plausibly useful to
calling code when deciding how to handle the error, without exposing
implementation details that would be burdensome to preserve.

Allow `Debug` implementations to expose unstable implementation details.
Debug-string formats are not ordinarily part of a crate's API contract.

### Choose the error representation for the caller

When affected callers already match or downcast an error type, or rely on
reporter-specific error data, preserve that path when it can naturally represent
the new failure. Do not create a parallel type or switch libraries merely to
follow a general preference.

When creating a new structured error, prefer `thiserror` to derive `Display` and
`Error` implementations. Do not rewrite an existing error implementation solely
to adopt `thiserror`. Implement the traits manually when necessary to provide a
clearer error message.

Use `anyhow` for non-structured application errors when the application will
report them rather than inspect them programmatically. For plugin, middleware,
and extension APIs that require a generic open-ended error type, prefer
`anyhow::Error` over `Box<dyn Error>` or a similar erased trait object.

Do not replace a structured library error with an opaque application error when
callers need its details to decide what to do.

### Keep each error message distinct from its source

Do not make an error's `Display` implementation repeat text emitted by its
source's `Display` implementation. Assume that the reporting boundary traverses
source chains unless tracing the affected reporting path proves otherwise.

Prefer an outer message that adds context while exposing the underlying error
through `source()`:

``` rust
#[derive(Debug, thiserror::Error)]
#[error("failed to load {path}")]
struct LoadError {
    path: std::path::PathBuf,
    #[source]
    reason: std::io::Error,
}
```

Interpolating the reason without exposing it as a source is dispreferred but
acceptable:

``` rust
#[derive(Debug, thiserror::Error)]
#[error("failed to load {path}: {reason}")]
struct LoadError {
    path: std::path::PathBuf,
    reason: std::io::Error,
}
```

Never both interpolate the reason and expose the same value as a source:

``` rust
#[derive(Debug, thiserror::Error)]
#[error("failed to load {path}: {reason}")]
struct LoadError {
    path: std::path::PathBuf,
    #[source]
    reason: std::io::Error,
}
```

## Author procedure

When writing or revising Rust error handling:

1.  Classify the target as first-party code or vendored-fork code. For a
    vendored fork, apply the convention map from `$work-in-vendored-forks`.
    Trace the call sites touched by the task and direct repository uses of any
    public error behavior the change would alter, stopping at the first handler
    or presenter on each path. Record only behavior the change must preserve.
2.  Identify whether invalid inputs can be made unrepresentable without
    introducing disproportionate wrapper-type ceremony.
3.  Classify each failure at the whole-program level. Return an error when the
    failure can arise from external state or input after all caller obligations
    have been satisfied, unless it is a qualifying unavoidable third-party
    panic. Panic when the failure necessarily means that in-process code broke
    an obligation or invariant.
4.  For each trait implementation, compare the panic conditions reachable while
    it and its dependencies satisfy their contracts with the trait's documented
    contract. When claiming the third-party-trait exception, establish direct
    upstream evidence or a clear crate-wide pattern of incomplete panic
    documentation.
5.  For each known third-party panic, inspect supported public APIs, error
    channels, versions, features, and targets for a reasonable alternative. Do
    not count `catch_unwind` as avoiding the panic.
6.  At each panic site and call to a panicking API, identify the caller-facing
    condition, more-private invariant, or qualifying third-party panic involved.
    Establish every callee panic condition through a check, type, or maintained
    invariant, or carry it into the current item's panic contract. Replace every
    `Option::unwrap()` with an `expect` message explaining why the value should
    be `Some`.
7.  For a panicking `Result`, use `expect` exactly when it adds information
    absent from the `Err`; otherwise use `unwrap`. Normally unwrap
    poisoned-mutex results rather than repeating that the mutex should not be
    poisoned. Never unwrap a recoverable third-party error under the
    third-party-panic exception.
8.  Document every caller-facing panic condition in `# Panics` unless the
    current item establishes that it cannot occur. Omit internal bug checks that
    depend on a more-private invariant violation. Document every accepted
    third-party panic as a condition under which the API may panic, and put a
    trait-specific exception on the implementation.
9.  At a library boundary, expose only the structured distinctions and data that
    callers can plausibly use to determine the error's disposition.
10. Preserve an existing path through an error type or reporter when it can
    naturally represent the new failure. If no compatible path fits and a new
    structured error is needed, prefer `thiserror`; implement `Display` and
    `Error` manually only when necessary for a clearer message. Use `anyhow` or
    an open-ended `anyhow::Error` only where callers do not need structured
    details.
11. Keep unstable implementation detail in `Debug` rather than committing public
    error structure or `Display` text to it.
12. Audit every wrapper and source link so each `Display` adds context without
    repeating its source.
13. Re-read the affected call paths and public signatures to catch external
    failures converted to panics after caller obligations are satisfied, panic
    conditions neither established nor carried upward, private bug checks
    exposed as caller contracts, unsupported trait panic conditions, avoidable
    third-party panics, useless error variants, redundant panic messages, and
    duplicated source text.

## Reviewer procedure

When reviewing Rust error handling, first perform the same task-local
classification, compatibility tracing, and dependency analysis an author should
have performed.

Then report a finding when code:

- leaves an efficiently checkable public invariant representable when it is
  stable, domain-significant, reused across call sites, or otherwise important
  enough to justify a distinct type;
- adds a one-off validated wrapper whose ceremony exceeds its safety benefit;
- panics for a failure that can arise from external state or input after every
  applicable caller obligation has been satisfied, without satisfying the
  unavoidable-third-party exception;
- returns an error for a condition that can only indicate a bug;
- omits a caller-facing panic condition from `# Panics`, including a callee's
  condition that the current item neither establishes as impossible nor carries
  into its own contract;
- documents as caller-facing a panic that can occur only after more-private
  implementation code violates an invariant it is responsible for maintaining;
- introduces a panic in a trait implementation, reachable while it and its
  dependencies satisfy their contracts, that the trait's documented contract
  does not permit, without satisfying the third-party-trait exception;
- infers that a third-party trait's panic documentation is incomplete from
  silence alone or from isolated omissions elsewhere in its crate;
- propagates a third-party panic despite a reasonable supported alternative on
  the affected versions, features, and targets;
- treats a local unwrap, a violated dependency precondition, or a hypothetical
  dependency bug as an unavoidable third-party panic;
- treats `catch_unwind` as making an operation non-panicking;
- omits `may panic` documentation for an accepted third-party panic or promises
  that the API will panic;
- uses `Option::unwrap()`;
- gives `Option::expect()` or `Result::expect()` a message that describes the
  observed failure rather than why the value should be `Some` or `Ok`;
- uses `Result::expect()` when its message adds nothing beyond the `Err`,
  including the usual poisoned-mutex case;
- uses `Result::unwrap()` when an `expect` message could add informative
  invariant context absent from the `Err`;
- exposes library error details too coarse for plausible caller decisions or too
  coupled to implementation details to maintain;
- replaces actionable structured library errors with opaque report-only errors;
- creates a parallel error type or switches libraries merely to follow a general
  preference when an error path already used by affected callers can naturally
  represent the new failure;
- rewrites an existing error implementation solely to adopt `thiserror`, or
  implements a new structured error manually when `thiserror` would provide an
  equally clear result;
- uses `Box<dyn Error>` or a similar erased trait object for an open-ended
  extension API where `anyhow::Error` provides the required open-ended
  representation;
- repeats a source error's `Display` text in an outer error that also returns it
  from `source()`; or
- relies on unstable `Display` output where `Debug` is the appropriate place for
  implementation detail.

Do not report a finding merely because code panics for a genuine whole-program
bug when every caller-facing panic condition is documented as required,
propagates a documented and genuinely unavoidable third-party panic under the
rules above, uses `Result::unwrap()` when the `Err` already provides all useful
context, implements `Display` and `Error` manually for greater clarity, leaves a
one-off invariant without a wrapper type, or exposes unstable implementation
detail through `Debug`.
