# stiles-lint-rules

Lint rules for crates of [stiles][stiles] rules. They check what a reader of
a rule needs and what stiles checks only at run time: the docs of each pass,
the options it reads, and the rule ids it reports.

A crate of rules runs them by naming this repository and a commit:

```toml
# .config/stiles.toml
[[lints]]
git = "https://github.com/aonyx-ai/stiles-lint-rules"
rev = "0123456789abcdef0123456789abcdef01234567"
```

## The rules

Each directory under `lints/` is one rule, and each documents itself with a
negative example and its fix. Every rule reports an error and reads the
syntax of one file.

- `lint.incomplete-pass-doc` reports a public lint pass whose docs lack a
  summary, a body, an `# Examples` section, or a `// DON'T` example with its
  `// DO` fix.
- `lint.undocumented-option` reports an option that a pass reads through
  `RuleOptions` and its docs do not name.
- `lint.undeclared-rule-id` reports a diagnostic whose rule id no
  `DeclaresRules` in the file declares.

## The stiles pin

Every rule builds against the one stiles release that `Cargo.toml` names.
Stiles refuses a plugin built against another release, so a project loads
these rules with the stiles that the pin names. `just pin` prints it.

[stiles]: https://github.com/aonyx-ai/stiles
