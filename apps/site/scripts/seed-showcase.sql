INSERT INTO posts (public_id, slug, title, summary, body_adoc, status, published_at)
VALUES (
    LOWER(
        HEX(RANDOMBLOB(4)) || '-' ||
        HEX(RANDOMBLOB(2)) || '-' ||
        '4' || SUBSTR(HEX(RANDOMBLOB(2)), 2) || '-' ||
        SUBSTR('89AB', (ABS(RANDOM()) % 4) + 1, 1) || SUBSTR(HEX(RANDOMBLOB(2)), 2) || '-' ||
        HEX(RANDOMBLOB(6))
    ),
    'asciidoc-showcase',
    'AsciiDoc feature showcase',
    'A comprehensive test of every AsciiDoc feature supported by the rendering pipeline.',
    '= AsciiDoc feature showcase

This post exercises every major AsciiDoc feature supported by the blog''s rendering pipeline: inline formatting, lists, code blocks with syntax highlighting, mathematics via KaTeX, tables, admonitions, and Korean text.

== Inline formatting

This is *bold*, _italic_, `monospace`, *_bold italic_*, and [.line-through]#strikethrough#. You can also write ^super^script and ~sub~script.

== Lists

=== Unordered

* First item
* Second item
** Nested item A
** Nested item B
*** Deeply nested
* Third item

=== Ordered

. Step one
. Step two
.. Sub-step A
.. Sub-step B
. Step three

=== Description list

CPU:: Central processing unit
RAM:: Random-access memory
SSD:: Solid-state drive

== Code blocks

=== Python

[source,python]
----
def fibonacci(n: int) -> int:
    """Return the n-th Fibonacci number."""
    if n <= 1:
        return n
    a, b = 0, 1
    for _ in range(2, n + 1):
        a, b = b, a + b
    return b
----

=== Rust

[source,rust]
----
fn fibonacci(n: u64) -> u64 {
    let (mut a, mut b) = (0u64, 1u64);
    for _ in 0..n {
        (a, b) = (b, a + b);
    }
    a
}
----

=== JavaScript

[source,javascript]
----
function fibonacci(n) {
  let [a, b] = [0, 1];
  for (let i = 0; i < n; i++) {
    [a, b] = [b, a + b];
  }
  return a;
}
----

=== Shell

[source,bash]
----
#!/bin/bash
for i in $(seq 1 10); do
    echo "Iteration $i"
done
----

=== SQL

[source,sql]
----
SELECT p.title, p.published_at
FROM posts p
WHERE p.status = ''public''
ORDER BY p.published_at DESC
LIMIT 10;
----

== Mathematics

=== Inline math

The identity stem:[e^{i\pi} + 1 = 0] unites five fundamental constants. The sum of the first stem:[n] natural numbers is stem:[\sum_{k=1}^{n} k = \frac{n(n+1)}{2}].

=== Display math

The Gaussian integral:

[stem]
++++
\int_{-\infty}^{\infty} e^{-x^2}\,dx = \sqrt{\pi}
++++

The Euler product for the Riemann zeta function:

[stem]
++++
\zeta(s) = \sum_{n=1}^{\infty} \frac{1}{n^s} = \prod_{p\;\text{prime}} \frac{1}{1 - p^{-s}}, \quad \Re(s) > 1
++++

=== Aligned equations

[stem]
++++
\begin{aligned}
\nabla \cdot \mathbf{E} &= \frac{\rho}{\varepsilon_0} \\
\nabla \cdot \mathbf{B} &= 0 \\
\nabla \times \mathbf{E} &= -\frac{\partial \mathbf{B}}{\partial t} \\
\nabla \times \mathbf{B} &= \mu_0 \mathbf{J} + \mu_0 \varepsilon_0 \frac{\partial \mathbf{E}}{\partial t}
\end{aligned}
++++

=== Theorem and proof

*Theorem (Bolzano–Weierstrass).* Every bounded sequence in stem:[\mathbb{R}^n] has a convergent subsequence.

*Proof.* We proceed by bisection. Given a bounded sequence stem:[\{x_k\}] contained in a closed box stem:[B_0], split stem:[B_0] into stem:[2^n] congruent sub-boxes. At least one sub-box contains infinitely many terms; call it stem:[B_1] and pick stem:[x_{k_1} \in B_1]. Repeat to obtain a nested sequence of boxes stem:[B_0 \supset B_1 \supset B_2 \supset \cdots] whose diameters shrink to zero. By the nested intervals theorem, stem:[\bigcap_{j=0}^{\infty} B_j = \{x^*\}], and the subsequence stem:[x_{k_j} \to x^*]. stem:[\blacksquare]

== 한글 지원

한글 텍스트가 올바르게 렌더링되는지 확인합니다. 본문에서 한글과 English가 자유롭게 혼용되어야 하며, *굵게*, _기울임_, `고정폭` 등의 서식도 한글에 적용되어야 합니다.

=== 수학에서의 한글

수학 표현 안에서 한글을 사용할 수 있습니다:

[stem]
++++
\text{정리: } \forall\,\epsilon > 0,\;\exists\,\delta > 0 \;\text{s.t.}\; |x - a| < \delta \implies |f(x) - f(a)| < \epsilon
++++

이 정의는 stem:[\text{연속}]의 stem:[\epsilon\text{-}\delta] 정의입니다.

== Tables

[cols="1,2,1", options="header"]
|===
| Language | Key feature | Year

| Python
| Readability and ecosystem
| 1991

| Rust
| Memory safety without GC
| 2015

| Haskell
| Pure functional with lazy evaluation
| 1990

| SQL
| Declarative data querying
| 1974
|===

== Admonitions

NOTE: This is a *note* — general information the reader should be aware of.

TIP: This is a *tip* — a helpful suggestion.

WARNING: This is a *warning* — something that could cause problems.

IMPORTANT: This is *important* — do not skip this.

CAUTION: This is a *caution* — proceed carefully.

== Block quotes

[quote, Donald Knuth, "The Art of Computer Programming"]
____
Premature optimization is the root of all evil.
____

[quote, Edsger Dijkstra]
____
Computer science is no more about computers than astronomy is about telescopes.
____

== Links

Visit the https://docs.asciidoctor.org/[Asciidoctor documentation] for the full language reference.

== Horizontal rule

Content above the rule.

''''''

Content below the rule.

== Literal and sidebar blocks

....
This is a literal block.
    Whitespace is preserved exactly as written.
        Including indentation.
....

****
This is a sidebar block. It is typically used for tangential content or asides that complement the main narrative.
****

== Summary

If every section above renders correctly — inline formatting, nested lists, syntax-highlighted code in multiple languages, inline and display mathematics (including Korean text in stem expressions), tables, admonitions, block quotes, and horizontal rules — the rendering pipeline is healthy.',
    'public',
    STRFTIME('%Y-%m-%dT%H:%M:%SZ', 'now')
);
