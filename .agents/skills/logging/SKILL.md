---
name: "logging"
description: >-
  Use this skill when writing or reviewing code that emits operational log
  messages or events, regardless of programming language. It covers choosing
  logging facilities and levels, placing events, defining structured fields and
  stable event identities, handling authentication and terminal events, and
  controlling log volume. Do not use it for log-handling infrastructure that
  only processes existing messages and emits none of its own.
---

# Operational logging

Emit useful operational events at the layer and severity that reflect their
meaning, without duplicating error handling or creating avoidable production
noise.

## Apply repository scope

For first-party code in this repository, apply this skill's rules directly and
authoritatively within the task's change and review boundaries. Do not weaken
them merely because nearby code follows a different practice.

If the target is inside a vendored fork, mirror, or downstream-maintained copy
of third-party source, use `$work-in-vendored-forks` first and apply the
upstream conventions it resolves within that fork. First-party code that only
calls, wraps, configures, tests against, or otherwise interacts with a
third-party dependency is not fork maintenance; apply this skill normally,
including all dependency-interfacing rules.

For a new first-party component, use the repository-designated logging
facility. If none is designated, choose a mature, ecosystem-standard facility
that integrates with the program's routing and filtering and supports native
structured attributes when practical. For a new Rust component, use `tracing`
and its ecosystem. When an existing component already uses another facility,
do not introduce a parallel facility through an isolated edit. Continue using
the component's facility for in-scope events unless the task explicitly
includes a migration; do not broaden an ordinary logging change into one.
Treat dependencies, features, initialization, handlers, sinks, exporters,
filters, and build and runtime configuration as hard integration constraints.

Decide event identities, fields, and severity independently from the logging
API. Give `info`, `warn`, `error`, and optional terminal events stable identities
when centralized collection, queries, aggregation, alerts, or other programmatic
use needs them; otherwise omit explicit identities. Reuse semantic field keys
for the same concepts and apply the level rules from the emitter's perspective.

## Core rules

### Choose the level from the event's operational meaning

Classify the event by what happened at the emitting layer:

- **`error`:** an intended operation failed because something went wrong. Use
  it for every nonterminal failure, however severe.
- **`warn`:** an operation behaved abnormally, may have produced an incorrect
  result, or created a condition that warrants attention before it becomes an
  error.
- **`info`:** a significant but normal state transition occurred.
- **Debug-like:** a state transition occurred that is useful for diagnosis but
  does not warrant `info` or a higher severity. It may be internal or externally
  visible. Emit it at `debug`; `debug` is the highest severity permitted for a
  purely internal transition.
- **Trace-like:** the event reports control flow or the current state of
  execution without representing a state transition. Emit it at `trace` when
  that level exists, and otherwise at `debug`.

Treat these as semantic categories even when the facility spells a level
differently, such as `warning` instead of `warn`. Keep the distinction between
debug-like and trace-like events when both use the `debug` level. Mapping a
trace-like event to `debug` does not turn it into a debug-like state
transition.

Require an externally visible consequence from the underlying operation for an
`info`, `warn`, or `error` event. Treat external visibility as necessary but not
sufficient for those levels: an externally visible transition may still belong
at `debug` when it lacks higher-level operational significance. Require a state
transition for a debug-like, `info`, `warn`, or `error` event. Treat an
operation completing or failing as a state transition. Use the trace-like
category only for execution-state observations that do not meet that threshold.

Do not choose a level merely because a value has an error-like type or a
message contains the word "error." Apply the abnormal-condition and
authentication rules below before assigning a severity.

In this skill, trace-like log events are distinct from distributed traces,
spans, stack traces, and other uses of the word "trace."

### Limit levels above error and classify terminal APIs

For new or changed events, use `error` for every failed operation, however
severe. A codebase or component may designate at most one additional severity for a
terminal condition: one whose disposition at the emitting layer intentionally
prevents the current operation from resuming normally. Ordinary propagation to
another decision layer is nonterminal. If no extra severity exists, use `error`.

When a facility offers several levels above `error`, such as syslog's
`LOG_CRIT`, `LOG_ALERT`, and `LOG_EMERG`, do not assign them separate meanings
in new code. Map nonterminal failures to `error` and terminal conditions to the
one locally designated level, if any. Preserve required mappings and untouched
legacy uses, but treat every other above-`error` level as deprecated: do not
give it an independent meaning or extend the taxonomy.

Treat the optional terminal severity as a refinement of `error` that inherits
all of its requirements. In the rules below, "higher-level" includes `info`,
`warn`, `error`, and this optional severity. Severity describes the event, not
whether an API call returns. Read the facility's contract and configuration
rather than inferring control flow from names such as `fatal` or `critical`.

**Severity-only calls emit an event and return.** For example, Python's
`logging.Logger.critical` records a critical event but does not terminate the
program. Prefer `error` unless `critical` is the component's one designated
terminal severity and the condition is actually terminal. Invoke any required
non-reporting termination at the same disposition boundary; never assume the
logging call provides it.

**Guaranteed terminal logging calls emit an event and then always invoke a
non-returning mechanism.** For example, Go's `log.Fatal` logs and then calls
`os.Exit(1)`. When that is the established idiom and termination is intended at
this layer, use the call as the single reporting and control-flow operation.
Do not log first or add another exit, abort, panic, or throw afterward. Treat
the call as non-returning from that point, even though a surrounding mechanism
may catch its panic or exception. Do not use it instead of ordinary failure
propagation merely to obtain a stronger severity.

For a call that logs and then panics or throws, inspect the configured recovery
and reporting path. If it will report the same failure, use a non-logging panic
or throw and let that boundary own the event. Mere recoverability is not the
problem; actual duplicate reporting is. For example, if a Go handler uses
`log.Panic` and recovery middleware reports the panic, use a plain panic and let
the middleware log it.

**Conditionally terminal logging calls may terminate in some configurations
and return in others.** An example is a logger configured to panic in a
development mode but only record the event in production. Analyze the call as
returning: its event and the code after it must be correct on that path. Never
rely on it when termination is required. Its optional terminal behavior does
not justify an above-`error` severity; the condition must independently be
terminal on every configured path. If it panics or throws, apply the
configured-reporter check above.

A component may implement terminal disposition with a severity-only event
followed by an exit or abort that produces no diagnostic. Treat that pair as one
disposition event. Do not precede a runtime-reported panic, exception, abort,
or equivalent terminal failure with a duplicate log event.

When a terminal event is the only diagnostic and the facility buffers
asynchronously, use its documented terminal or flush path or a synchronous
sink. Do not assume that invoking the logging method made the record durable,
and do not use an arbitrary delay as a flush mechanism. This is a delivery
requirement, not a prediction about unwinding or general cleanup.

### Add debug-like and trace-like events only for a concrete diagnostic need

Before adding a debug-like event, identify a plausible diagnostic question or
recurring failure mode that the event would help resolve. Do not add debug-like
events merely because a location might someday be interesting.

Add trace-like events only while investigating a concrete problem. Before
completing that investigation, remove the trace-like events introduced for it,
including those emitted at `debug` because no `trace` level exists. Do not
remove pre-existing trace-like instrumentation outside the task's scope merely
because it does not appear useful to the current investigation.

After removing investigation-only events, identify whether one state transition
would have pointed directly to the problem. If a corresponding debug-like event
is likely to help with the same class of problem again, add that event.

### Log abnormal conditions at the layer that disposes of them

Choose one layer to decide the disposition of an abnormal condition. Treat code
as propagating the condition when it hands the condition to another layer that
still decides how to handle or report it. Propagation may use an error or
failure return, a thrown or rethrown exception, a rejected promise, an error
continuation, or another mechanism. Do not also emit an `info`, `warn`, `error`,
or optional terminal event for the same condition.
Wrapping or translating the failure before propagating it does not change this
rule.

Do not classify a return as propagation from syntax alone. A boundary may own
disposition when it returns a final process status, protocol response, or
equivalent result to a runtime or framework that only transports that decision
and will not report it independently. That boundary may emit the one disposition
event. If the receiving runtime or framework will report the condition, leave
the event to that reporting boundary.

Code may add purposeful debug-like or trace-like context and still propagate
the failure because those diagnostic events do not dispose of the condition.

Do not assume every error-shaped value or exception is abnormal in the
application that receives it. Reusable library code that lacks enough
application context to determine a failure's operational significance must
leave higher-level disposition to its caller. Such code may emit debug-like or
trace-like events when they meet the other rules in this skill. When no evidence
establishes that a library owns higher-level operational events, default it to
diagnostic events and leave higher levels to the application.

A guaranteed terminal logging call is a special case. When its terminal
behavior is itself intended and no configured recovery path will duplicate its
event, treat the call as one reporting and control-flow operation. Do not split
it into a separate log event and propagated failure for purposes of the rule
above. Apply the ordinary rule to the returning path of a conditionally
terminal operation.

### Do not duplicate terminal failures in logs

When a panic, uncaught terminal exception, assertion failure, abort, or similar
runtime mechanism will report a failure, do not emit a log event immediately
beforehand that duplicates the report. A guaranteed terminal logging call is
itself the reporting and termination mechanism and does not violate this rule.

At a top-level disposition boundary, use the designated terminal severity, or
`error` if none exists, for one event before a non-reporting termination only
when that is the established design and execution remains trustworthy. If a
bug-check indicates heap or allocator corruption, or comparable corruption that
may make logging unsafe, abort directly and as quickly as possible without
logging. Do not format values, allocate, acquire logger locks, serialize, or run
logging hooks first. Treat the absence of a terminal diagnostic as correct.

### Preserve dynamic semantic data as structure when possible

When the logging facility supports fields, attributes, properties, or named
message-template arguments that the backend preserves as separately queryable
data, record every dynamic fact with semantic significance through that
structured mechanism. A human-readable message may repeat or format those
facts, but it must not be the only place where they appear.

A presentation-only value need not have its own field when it is determined
entirely by a recorded field. The following Rust `tracing` example illustrates
this language-independent distinction; use the semantic equivalent offered by
the target language and logging facility:

``` rust
let s = if number == 1 { "" } else { "s" };
info!(name: "blegs_frobnicated", number, "frobnicated {number} bleg{s}");
```

Here `number` carries the semantic data. The unrecorded `s` only makes the
message grammatical and introduces no independent information.

If the backend preserves only rendered text, apply the string fallback even
when the call accepts format or template arguments. First follow an established
component encoding. During an isolated edit to an existing component, preserve
its record shape rather than emitting one JSON-shaped outlier. For a new
component, or when defining an encoding is explicitly in scope, centralize
machine-consumed text encoding at the logging boundary and use a standard
serializer to emit one compact JSON object as the message payload.

Use a stable envelope with `message`, an optional `event`, and a nested `fields`
object so semantic keys cannot overwrite envelope keys. Include severity and
timestamp when the outer facility does not preserve them. Verify that the
downstream framing can isolate the payload despite logger prefixes or wrapping,
and preserve each object as one record. Choose serializable field
representations and provide a safe minimal fallback if encoding fails; logging
must not replace the primary failure with a serialization failure. Let the
serializer escape newlines and other delimiters. Do not hand-build JSON or
invent a different encoding at each call site.

For string-only logs intended solely for local human diagnosis, use stable prose
with dynamic facts explicitly and consistently labeled. Do not describe those
labels as queryable structured fields. Do not introduce a parallel logging
facility or a one-off abstraction solely to give one isolated event native
structure.

### Apply one explicit-event-identity policy consistently

Use one of these policies throughout a component:

1. Leave debug-like and trace-like events unnamed, and assign explicit stable
   identities to every `info`, `warn`, and `error` event and to any
   optional terminal event.
2. Do not assign explicit identities to any events.

Use the first policy when higher-level events need stable identities for
centralized collection, queries, aggregation, alerting, or other programmatic
consumption. Use the second when logs serve only as local, human-readable
diagnostics. Do not merge distinct terminal event identities merely because
they share one severity; preserve their causes and mechanisms in identities and
fields.

Prefer a facility's native event name, ID, or template ID. If native identities
are unavailable but structured fields exist, use a consistent field such as
`event`. Carry the same property in the shared machine-readable encoding for a
string-only facility. Do not add artificial identity prefixes to human-only
messages under the unnamed policy.

### Name fields for consistent queries

Reuse the same key for the same concept throughout the relevant logging domain.
Start with the least-qualified key that remains unambiguous in the emitting
component, and add at most one contextual qualifier when necessary.

For example, a general-purpose filesystem component may use `file`, while a
configuration parser may need `config_file`. Do not repeat context already
implied by the component's purpose: source tooling should prefer `src_file` to
a language-specific key such as `rust_src_file`.

Represent independent dimensions as separate fields instead of adding multiple
qualifiers to one key. If source tooling supports multiple languages, keep
`src_file` and add a separate field such as `language = "rust"`.

Apply the same semantic keys to labeled string values and machine-readable
fallback encodings.

### Decide logging separately from user-interface feedback

Decide independently whether a condition requires immediate user feedback and
whether it is operationally useful to log.

Do not log invalid input that a user-interface or argument-parsing framework
handles completely without taking an action. Never rely on a log message to
provide required user feedback. Giving the user feedback and logging the same
underlying condition is acceptable when each communication has an independent
purpose; duplication between the two is not itself a defect.

### Classify authentication from the emitter's perspective

On a client, treat an authentication failure as an `error` when it prevents the
requested operation and the emitting layer owns the failure's disposition.

On a server, successfully rejecting invalid credentials means the server
behaved as designed. If an individual rejection has diagnostic or
administrative value, record it at `debug` or `info` according to that value;
otherwise omit it. Never record a correct rejection at `warn` or `error`.

Record every accepted server authentication at least as severely as a rejected
authentication. Use `info` for routine successful logins. Use `warn` when a
successful login is itself abnormal enough to demand attention, such as access
to an administrator account that should be used rarely.

Distinguish a correct credential rejection from a failure of the authentication
system, such as an unavailable identity provider. Classify a system failure
under the ordinary level rules, and emit its disposition event only at the
layer that stops propagating it.

### Bound production log volume

Before adding an event on a request, input item, retry, or other potentially
unbounded path, evaluate how many events attacker-controlled or worst-case
traffic can produce. Treat the change from no per-item event to any per-item
event as the first scaling boundary, then account for the number and size of
events each item produces.

If a fact is useful only in aggregate, prefer recording it as a metric through
the project's metrics facility. When human-readable reporting is also useful,
consider logging aggregate summaries at slow, regular intervals. Individual
debug-like events may remain at `debug` when production deployments normally
disable that level; trace-like investigation events must still be removed.

## Author procedure

1. Classify the target as first-party code or vendored-fork code. For a vendored
   fork, apply the convention map from `$work-in-vendored-forks`. For a new
   first-party component, use the repository-designated facility, or a mature
   ecosystem-standard facility with native structure when practical; use
   `tracing` for Rust. When native structure is unavailable, use the compatible
   conventional facility and apply the text fallback. Retain an existing
   component's facility unless migration is explicitly in scope.
2. Resolve structured-data, event-identity, severity, and terminal behavior,
   including the optional terminal severity and every returning path. Inspect
   configured recovery and reporting paths; do not infer from method names.
3. Decide separately whether the condition belongs in a log, in user-interface
   feedback, in a metric, or in none of them.
4. Identify the state transition or trace-like execution-state observation that
   the event represents. Treat an operation completing or failing as a state
   transition.
5. For an abnormal condition, identify the layer that owns its disposition. Do
   not emit a higher-level event from code that propagates the same condition.
6. Choose the level from operational meaning and visibility. Reserve the
   optional above-`error` severity for terminal conditions. Emit trace-like
   events at `debug` when `trace` is unavailable. Use terminal logging calls
   only when their control flow is intended and logging remains safe.
7. Evaluate event volume under worst-case and attacker-controlled inputs.
   Prefer metrics and periodic summaries to per-item events for aggregate-only
   information.
8. Preserve every dynamic semantic fact through native structure when
   available; otherwise apply the established string encoding, shared JSON
   encoding, or human-only labeled-message rule.
9. Apply the event-identity and field-key policies consistently, distinguishing
   terminal causes there rather than with extra severities.
10. For authentication events, classify the outcome from the client or server's
    perspective and ensure successful server authentications are recorded at
    least as severely as rejections.
11. Remove trace-like events introduced for a completed investigation, including
    those mapped to `debug`, then consider whether one durable debug-like state
    transition would make the same class of problem easier to diagnose.
12. Re-read the call path and remove duplicate disposition logs, deprecated
    above-`error` distinctions, ambiguous encodings, and events that no longer
    meet their level's threshold.

## Reviewer procedure

When reviewing logging, first perform the same target classification,
facility-scope check, capability analysis, and contextual event-identity,
field, and severity decisions an author should have performed.

Then report a finding when code:

- uses a facility inconsistent with the rules for a new first-party component,
  introduces a parallel facility through an isolated edit, or migrates an
  existing component without explicit task scope;
- applies an explicit-event-identity policy inconsistent with the component's
  operational consumption;
- introduces more than one semantic level above `error`, uses the optional
  terminal severity on a disposition path that resumes normally, or otherwise
  assigns the wrong level;
- emits a higher-level event without an externally visible consequence;
- emits a debug-like or higher-level event without a state transition,
  including an operation completing or failing;
- adds a debug-like event without a plausible diagnostic use or leaves
  investigation-only trace-like events in the completed change, including
  trace-like events mapped to `debug`;
- emits a higher-level event for an abnormal condition and also propagates that
  condition;
- mistakes a severity-only or conditionally terminal call for non-returning,
  treats a guaranteed terminal call as returning, or substitutes terminal
  logging for ordinary non-terminal failure propagation;
- uses a panic- or throw-logging call when a configured recovery,
  supervision, or runtime path will emit the equivalent event;
- emits duplicate events around a guaranteed terminal logging call or immediately
  before a runtime-reported terminal failure;
- invokes a logging facility from a bug-check after detecting corruption that
  may make logging itself unsafe;
- emits higher-level events from reusable library code that lacks the context
  to determine application-level significance;
- places dynamic semantic information only in a message when the facility can
  preserve it as native structure;
- uses an ambiguous, hand-built, or per-call encoding for machine-consumed
  string-only logs;
- violates the component's event-identity policy or established field-key
  taxonomy;
- encodes multiple independent dimensions as qualifiers in one field key;
- logs invalid input that the user interface handles completely without taking
  an action;
- relies on a log event to provide required user feedback;
- records an individual server-side authentication rejection at `warn` or
  `error`;
- treats an authentication-system failure as a credential rejection or emits a
  higher-level event before propagating it;
- omits an accepted server authentication or records it less severely than a
  rejection; or
- adds potentially unbounded production logging without accounting for
  worst-case volume, especially when the information is meaningful only in
  aggregate.

Be conservative about requests to add logging. Do not require an event merely
because an error-shaped value exists or an event might occasionally be useful.

Do not report a finding merely because:

- user-interface feedback and a log event describe the same condition for
  independent purposes;
- an explicit event identity is absent under the unnamed policy;
- reusable library code limits itself to purposeful diagnostic events;
- a trace-like event uses `debug` because the facility has no `trace` level;
- a guaranteed terminal logging call is the component's idiomatic terminal
  mechanism and no configured reporting path duplicates it;
- a serious corruption check aborts without logging because logging may be
  unsafe;
- a severity-only terminal event is immediately paired with the intended
  non-reporting terminal action;
- a string-only human-facing message uses stable labeled values instead of
  unavailable native fields; or
- pre-existing trace-like instrumentation exists outside the change's scope.

## Logging stance

- Use the repository-designated or ecosystem-standard facility for new
  components, preferring native structure when practical and applying the text
  fallback when it is unavailable. Use `tracing` for Rust, and retain an
  existing component's facility unless migration is explicitly in scope.
- Prefer one disposition event over repeated logs along a failure's propagation
  path.
- Use `error` as the ceiling for nonterminal failures and at most one higher
  severity for terminal conditions. Distinguish terminal causes through event
  identities and fields, not more severity levels. Treat an idiomatic guaranteed
  terminal logging call as one reporting and control-flow operation.
- Prefer native structured fields, then a shared machine-readable encoding,
  over message-only semantic data; use stable labeled prose for human-only
  string facilities.
- Prefer a durable debug-like state transition over permanent trace-like
  control-flow logging, even when both map to `debug`.
- Prefer metrics over high-volume events that matter only in aggregate.
- Prefer the emitter's operational perspective over severity inferred from
  generic labels such as "error" or "authentication failure."
