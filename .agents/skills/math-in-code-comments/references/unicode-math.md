# UnicodeMath reference

Use this reference after the main decision procedure selects UnicodeMath.

## Official publication

- Title: *UnicodeMath, A Nearly Plain-Text Encoding of Mathematics*
- Publication: Unicode Technical Note \#28
- Author: Murray Sargent III
- Version: 3.3
- Date: January 22, 2025
- [Version 3.3 landing page](https://www.unicode.org/notes/tn28/tn28-7.html)
- [Version 3.3
  PDF](https://www.unicode.org/notes/tn28/UTN28-PlainTextMath-v3.3.pdf)
- [Latest-version landing page](https://www.unicode.org/notes/tn28/)
- PDF size: 1,392,359 bytes
- Independently computed PDF SHA-256:
  `be092e3217ab15eca2d4bbbb28873d86614fc7357a399993903e018173909634`

The checksum identifies the file inspected while preparing this skill; it is not
a publisher-authenticated checksum.

This repository intentionally links to the publication instead of redistributing
it. The [Unicode Terms of Use](https://www.unicode.org/copyright.html) restrict
public distribution of copies without separate permission.

## Find the needed syntax

Consult:

- Sections 2 and 3 for expression syntax;
- Section 3.9 for matrices;
- Section 3.22 for the compact construct summary;
- Appendix A for the grammar;
- Appendix B for character and control-word lookup; and
- Section 6 for discussion of UnicodeMath in programming languages.

Read the PDF rather than guessing when a formula uses matrices, equation arrays,
prescripts, above/below scripts, accents, arbitrary groupings, or other
specialized constructs.

## Use the core notation

Use these common constructs:

| Meaning                   | UnicodeMath      |
|---------------------------|------------------|
| Fraction                  | `a/b`            |
| Grouped fraction          | `(a+b)/c`        |
| Superscript               | `x^2`            |
| Subscript                 | `a_i`            |
| Subscript and superscript | `a_i^2`          |
| Square root               | `√x` or `√(x+y)` |
| Bounded sum               | `∑_(i=1)^n a_i`  |
| Bounded integral          | `∫_0^a f(x)ⅆx`   |
| Two-by-two matrix         | `■(a&b@c&d)`     |

Use parentheses to override linear-format precedence:

- `α + β/γ` means that only `β` is divided by `γ`.
- `(α + β)/γ` puts the complete sum in the numerator.

A quadratic-formula example is:

`x = (-b ± √(b^2 - 4ac))/(2a)`

Spaces can terminate operands and carry syntax in UnicodeMath. Preserve or omit
them deliberately, especially after n-ary operators and around grouped
expressions.

## Produce final comment text

Use the actual character represented by a control word when practical. For
example, use `∑`, `√`, `→`, and `≥` in the final formula rather than leaving
input shortcuts such as `\sum`, `\sqrt`, `\to`, or `\ge`.

Keep ASCII syntax characters when UnicodeMath assigns them structural meaning,
including `/` for a fraction and `_` and `^` for scripts. Do not replace them
mechanically with typographic lookalikes.

Preserve the formula's semantics over visual ornament. If the code uses ASCII
identifier names while the explanatory formula uses conventional mathematical
symbols, define the correspondence in nearby prose.
