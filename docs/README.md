# The J3 Book

Human-readable docs for the J3 language and `j3c`, built with [mdBook](https://rust-lang.github.io/mdBook/) in the Rust book style.

    mdbook serve docs      # live preview at http://localhost:3000
    mdbook build docs      # static site in docs/book/

Pages are Markdown in `src/`; the table of contents is `src/SUMMARY.md`. Code blocks use the `rust` highlighter because highlight.js has no J3 grammar.

`j3c doc` stays the source of truth for exact stdlib names; this book explains, it does not list every function.
