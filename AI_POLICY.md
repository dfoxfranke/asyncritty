# AI Policy

This project welcomes all high-quality contributions regardless of authorship
method. However, we have some rules regarding the use of AI models. They
basically come down to two things:

1. Humans must be the face of all communication with project maintainers and
   assume final responsibility for all contributions.

2. Tool-assisted contributions must include commit trailers noting what tools
   were used.

## Rule 1: Human communication and responsibility

**Automated communication with project maintainers through the use of AI agents
is prohibited.** This prohibition includes automated submission of pull
requests, issues, issue comments, emails to maintainers, social media
communication, etc.

Issues and PR summaries must be human-authored. AI-written passages may be
included sparingly if they are block-quoted and attributed, but such passages
must not make up the bulk of the communication.

If you are not comfortable communicating in English, post in your native
language and then follow it with a block-quoted AI translation. Issue titles and
the summary line of commit messages must be English-only; if you use an AI to
generate just these, that's fine and there's no need to post any disclosure
stating that you did so.

**Humans are expected to have thoroughly reviewed all AI-authored contributions,
to be able to vouch for and assume responsibility for their quality, and to be
able to answer questions from maintainers without needing to forward those
questions to an AI.**

It happens to everybody that sometimes we didn't really review AI code as
carefully as we thought we did, and that the AI did things that we didn't
realize. This is fine as long as it is not too frequent. When it happens, you
are expected to be open about it.

## Rule 2: Commit trailers recording tool assistance

**Contributions substantially assisted by AI or other specialized tools must
include an `Assisted-by` trailer on their commit message noting them.** Tools
that should be mentioned include:

* AI models
* Static analyzers
* Fuzzers
* `cargo fix`

but not:

* `cargo fmt` / `rustfmt`
* `cargo clippy`
* `rust-analyzer`
* Anything else similarly basic to the Rust ecosystem

Try to keep your spelling of tool identities as consistent as possible with
other contributors. For AI models, use the form `harness:vendor/model` and take
the `vendor/model` identity from [models.dev](https://models.dev) if possible.
For everything else, use `tool:version`. Examples:

* `Assisted-by: codex-cli:openai/gpt-5.5 cargo-fix:1.96`
* `Assisted-by: claude-code:anthropic/claude-sonnet-4-5`
* `Assisted-by: cursor:xai/grok-4.3`

A contribution may be substantially assisted by a tool even if the tool didn't
generate any of the code. When in doubt, include it, but don't go overboard: the
trailer is about *substance*, not microscopic contagion.

## Additional Guidelines

LLMs are great at following instructions, but not so great at architectural
judgement or interface design. They do their best work when they're given a
stubbed-out API and a crisp specification to work against. If you're planning a
large contribution, you're encouraged to develop it this way and to show your
work by splitting the human-written scaffolding and then the AI-written
implementation into separate commits.

LLMs are not good at drafting prose. It's best to write documentation yourself
and restrict the AI's role to copy-editing. Contribution of AI-authored
documentation is not prohibited but is likely to be met with skepticism.

These guidelines reflect the mid-2026 state of the art, and will evolve as
capabilities do.
