---
name: "unicode-source-safety"
description: >-
  Determine whether literal non-ASCII Unicode text can be written reliably
  in an exact program-source or documentation-source file across every
  supported reader, compiler, interpreter, formatter, renderer, build,
  documentation, and code-generation path. Use this skill when writing or
  review requires a source-encoding and transport decision for Unicode in
  comments, documentation comments, standalone documentation, literals,
  identifiers, or other source regions; do not use it to decide grammar,
  semantics, style, escaping, normalization, or Unicode security.
---

# Unicode source safety

Decide whether an exact program-source or documentation-source file and its
supported processing path can carry literal non-ASCII text reliably. Use the
result as one input to the calling task, not as permission to use Unicode in
every syntactic or semantic context.

## Apply repository scope

For first-party source in this repository, apply this skill's safety criteria
directly and authoritatively within the task's assessment and change boundaries.
Existing literal Unicode is evidence about bytes, not permission to relax the
criteria.

If the target is inside a vendored fork, mirror, or downstream-maintained copy
of third-party source, use `$work-in-vendored-forks` first. Assess both the
upstream-supported source-processing paths and the host repository's import,
patch, build, documentation, generation, and other integration paths. Return
`Safe` only when every path in both sets preserves and decodes the text
consistently.

First-party code or documentation that only calls, wraps, configures, tests
against, or otherwise interfaces with a third-party dependency is not fork
maintenance. Apply this skill normally. Include a dependency tool in the
source-path analysis when it actually reads or rewrites the target file, but do
not invoke `$work-in-vendored-forks` merely because a dependency participates in
processing.

## Return a binary result

Return one of these conclusions for the exact target file:

- **Safe:** every supported path preserves and decodes the non-ASCII source text
  consistently.
- **Unsafe:** at least one supported path does not, or the available evidence is
  insufficient to establish safety.

State the controlling evidence or missing guarantee with the conclusion. Do not
return `Conditional`; that classification in the table below identifies a
condition to investigate. If the condition cannot be established, return
`Unsafe`.

Do not select, report, or recommend an escaped representation. The caller owns
that separate lexical and formatting decision.

A `Safe` result covers source-text encoding and transport only. Separately check
whether the programming language or documentation grammar permits the characters
in the intended location and whether their use is semantically correct and
consistent with project policy. Do not infer that Unicode is appropriate in
identifiers, string or protocol values, regular expressions, directives, or
generated output. Treat normalization, bidirectional text, confusables, and
other Unicode security or style concerns as separate decisions.

## Assess the exact source path

1.  Identify the target file's programming language or documentation format,
    version or dialect, encoding declarations or byte-order mark, and effective
    file, subtree, build, and toolchain encoding constraints.
2.  For documentation embedded in program source, classify literal text under
    the outer programming language. An inner documentation grammar does not make
    the enclosing source encoding safe.
3.  Identify every supported path that reads or rewrites the file, including
    editors or importers when prescribed, compilers, interpreters,
    preprocessors, transpilers, formatters, build tools, documentation
    generators, renderers, code generators, and relevant host-version
    combinations.
4.  Apply explicit file, repository, and build policies. An ASCII-only or legacy
    encoding restriction makes the result `Unsafe`. An explicit, enforced
    end-to-end Unicode encoding can establish `Safe` even when the generic
    language baseline is unsafe, but only when it covers every supported path.
5.  Otherwise, use the tables below. Apply their version and environment
    qualifications to the actual target.
6.  For an unlisted language or source type, apply the fallback rule below.

Assess the current supported path. Do not add or change a byte-order mark,
encoding declaration, compiler option, or supported-host constraint merely to
obtain `Safe` unless the calling task authorizes that broader change.

Do not treat existing non-ASCII text, runtime Unicode string support, Unicode
escapes, editor defaults, locale-dependent multibyte behavior, or one compiler
accepting the bytes as proof of a safe source path.

## Apply the normative fallback

Absent an explicit end-to-end project guarantee, treat non-ASCII source text as
safe only when an authoritative language or documentation-format specification
or reference does at least one of the following:

- define source text as Unicode characters, scalar values, or code points; or
- give source files a specific, well-defined Unicode encoding.

If authoritative evidence is missing or ambiguous, return `Unsafe`.

## Classify common source types

### Programming languages

Use this table as the language baseline. It explicitly covers the languages in
the [RedMonk January 2025 top
20](https://redmonk.com/sogrady/2025/06/18/language-rankings-1-25/) and
additional common or instructive cases. Apply more-specific file and toolchain
evidence before producing the final result.

| Language or source type | Baseline | Qualification |
|----|----|----|
| JavaScript / ECMAScript | Safe | Source text is defined in Unicode code points. |
| TypeScript | Safe | Honor a more-specific project or file encoding. |
| Python 3 | Safe | An explicit legacy coding cookie makes that file unsafe. |
| Java | Safe | An explicit non-Unicode compiler source encoding makes the build unsafe. |
| C# | Safe | Honor a more-specific project or file encoding. |
| Go | Safe | Source is defined as UTF-8 Unicode text. |
| Rust | Safe | Source must be valid UTF-8. |
| Kotlin | Safe | Honor a more-specific project or file encoding. |
| Swift | Safe | Comments are defined over Unicode scalar values. |
| Ruby 2.0 and later | Safe | A magic comment declaring a legacy encoding makes that file unsafe. |
| Dart | Safe | Source text is defined in Unicode code points. |
| Scala 2 and 3 | Safe | Source is defined as Unicode text. |
| Haskell | Safe | The language reports define the Unicode character set. |
| Julia | Safe | Source uses UTF-8. |
| C | Unsafe | Physical source mapping remains implementation-defined, including in C23 and current drafts. |
| C++ | Unsafe | Physical source acceptance and mapping remain implementation-defined, including in C++23. |
| Objective-C / Objective-C++ | Unsafe | These inherit the C or C++ physical-source model. |
| PHP | Unsafe | Its source and string model does not establish a Unicode source encoding. |
| R | Unsafe | Source interpretation can depend on the session encoding and locale. |
| POSIX shell / Bash / Zsh | Unsafe | Parsing is byte- and locale-dependent rather than fixed to Unicode. |
| Lua | Unsafe | Source is byte-oriented; the UTF-8 library does not redefine source text. |
| Generic SQL | Unsafe | Encoding depends on the dialect, client, server, and transport. |
| Generic assembly | Unsafe | Encoding depends on the assembler and object-building path. |
| CSS | Conditional | Require an effective UTF-8 encoding with no legacy referring-document or transport override. |
| Python 2 | Conditional | Require an applicable UTF-8 encoding declaration. |
| Ruby before 2.0 | Conditional | Require an applicable UTF-8 magic comment. |
| Perl | Conditional | Require a UTF-8 BOM or an applicable `use utf8` declaration for the source region. |
| PowerShell | Conditional | Require a PowerShell 6-or-later-only target with effective UTF-8, or a BOM-backed Unicode encoding that works on every supported host; account separately for a Unix shebang. |
| MATLAB | Conditional | Require R2020a or later using the UTF-8 default, or another explicit end-to-end UTF-8 toolchain. |

For an unsafe baseline, do not promote a file merely because a particular
compiler, interpreter, or editor accepts UTF-8. Require an explicit project
policy and configuration that fixes the encoding throughout every supported
source-processing path.

### Documentation formats

Use the file's actual configured format rather than inferring it solely from an
extension. `Conditional` baselines remain unsafe until their qualification is
established.

| Documentation source type | Baseline | Qualification |
|----|----|----|
| Plain text | Conditional | Safe for new files, and for existing files if the file appears to use an encoding capable of representing Unicode. Assume UTF-8 in the absence of contrary evidence. |
| [CommonMark](https://spec.commonmark.org/0.31.2/#characters-and-lines) and CommonMark-based Markdown | Safe | CommonMark defines characters as Unicode code points. An encoding-limited parser or a more-specific file or transport encoding makes the actual path unsafe. |
| reStructuredText with [Sphinx](https://www.sphinx-doc.org/en/master/usage/configuration.html#confval-source_encoding) or [Docutils 0.22 and later](https://docutils.sourceforge.io/docs/user/config.html#input-encoding) | Safe | Sphinx defaults source files to `utf-8-sig`, and Docutils 0.22 and later defaults input to UTF-8. Older versions, other processors, and configured legacy encodings are conditional. |
| AsciiDoc with current [Asciidoctor](https://docs.asciidoctor.org/asciidoc/latest/normalization/) | Safe | Asciidoctor assumes UTF-8. Other processors and includes with a different encoding are conditional. |
| [Current conforming HTML](https://html.spec.whatwg.org/multipage/semantics.html#charset) | Safe | Current HTML requires UTF-8. Legacy or nonconforming tools and transport metadata can make the actual path unsafe. |
| [XML 1.x](https://www.w3.org/TR/xml/#charencoding), including XML DocBook and DITA | Safe | XML processors must accept UTF-8 and UTF-16, and an entity without external encoding information, a byte-order mark, or an encoding declaration defaults to UTF-8. A declared legacy encoding or conflicting transport metadata makes the actual path unsafe. |
| [LaTeX from the 2018 release onward](https://latex-project.org/news/latex2e-news/) | Safe | UTF-8 is the default input encoding. Plain TeX, older LaTeX, compatibility rollback, and configured legacy input encodings are conditional. |
| [Generic roff or man source](https://www.gnu.org/software/groff/manual/groff.html.node/Input-Format.html) | Unsafe | Input encoding depends on the formatter and preprocessing path. An explicit end-to-end Unicode configuration can promote the actual path. |
| [GNU Texinfo](https://www.gnu.org/software/texinfo/manual/texinfo/html_node/_0040documentencoding.html) | Safe | UTF-8 is the default input and output encoding. An explicit legacy `@documentencoding` or an incompatible output path makes the actual path unsafe. |
| Org | Conditional | Require an effective Unicode file coding system across every supported [Emacs and export path](https://www.gnu.org/s/emacs/manual/html_node/emacs/Coding-Systems.html). |
| [POD](https://perldoc.perl.org/perlpod) | Conditional | Require an applicable Unicode byte-order mark or encoding declaration and compatible supported formatters; encoding guesses are insufficient. |
