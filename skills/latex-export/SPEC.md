# LaTeX Export

## Purpose
Convert the document body from markdown to LaTeX source, ready to feed into `pdflatex` / `xelatex` / `tectonic`. Useful for academic writing, formal reports, anything requiring LaTeX's typography.

## Capabilities
- `read_document` — needs the body to convert.

## Actions
| action_id | display label | params | output variant |
|---|---|---|---|
| `export` | Export as LaTeX | JSON: `{"document_class": "article", "standalone": true}` (both optional) | `File` |

## Sample input → output

`export` on:
```
# My Section

Some **bold** text with a [link](https://example.com).

- item 1
- item 2
```

returns a `.tex` file with body:
```latex
\documentclass{article}
\usepackage{hyperref}
\begin{document}

\section{My Section}

Some \textbf{bold} text with a \href{https://example.com}{link}.

\begin{itemize}
\item item 1
\item item 2
\end{itemize}

\end{document}
```

If `standalone: false`, omits the `\documentclass` preamble and `\begin{document}/\end{document}` wrappers — emits a fragment suitable for `\input{}`.

## Complexity estimate
Medium. Mapping markdown constructs to LaTeX is mostly mechanical, but escaping is finicky — characters like `&`, `_`, `%`, `$`, `#`, `\`, `{`, `}` all need backslash escaping in LaTeX prose.

## Host-db extensions needed
None.

## Suggested implementation notes
- Walk events from `pulldown-cmark` and emit LaTeX equivalents:
  - Heading levels → `\section`, `\subsection`, `\subsubsection`, `\paragraph`, `\subparagraph`, `\subparagraph` (LaTeX article class only goes to subparagraph; H6 falls back).
  - Code blocks → `\begin{verbatim}...\end{verbatim}`. For syntax highlighting, recommend `listings` or `minted` packages — leave that as a future param.
  - Inline code → `\texttt{...}` (escape the contents).
  - Emphasis / strong → `\emph{}` / `\textbf{}`.
  - Links → `\href{url}{text}`. Bare URLs → `\url{...}`.
  - Lists → `itemize` / `enumerate`.
  - Tables → `tabular`. Pandoc-style alignment from the GFM table syntax.
- Escape function: replace `& % $ # _ { } ~ ^ \` with their LaTeX-safe forms. Carefully ordered — escape `\` first.
- Default `document_class`: `"article"`. Other reasonable values: `"report"`, `"book"`, `"memoir"`.
- Output filename: `<sanitized title>.tex`.
