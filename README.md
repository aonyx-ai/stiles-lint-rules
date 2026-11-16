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
negative example and its fix. Every rule reports an error and reads one
file. `lint.untested-fix` also reads the decorations that mark test code.

- `lint.incomplete-pass-doc` reports a public lint pass whose docs lack a
  summary, a body, an `# Examples` section, or a `// DON'T` example with its
  `// DO` fix.
- `lint.undocumented-option` reports an option that a pass reads through
  `RuleOptions` and its docs do not name.
- `lint.undeclared-rule-id` reports a diagnostic whose rule id no
  `DeclaresRules` in the file declares.
- `lint.untested-fix` reports a fix that a crate root attaches when no test
  in that file applies a fix.

`lint.untested-fix` reads `fix-assertions`. It names the functions whose
call in a test applies a fix, and it replaces the default, `assert_fix`:

```toml
[rules.options."lint.untested-fix"]
fix-assertions = ["assert_fix", "assert_example"]
```

## The stiles pin

Every rule builds against the one stiles release that `Cargo.toml` names.
Stiles refuses a plugin built against another release, so a project loads
these rules with the stiles that the pin names. `just pin` prints it.

[stiles]: https://github.com/aonyx-ai/stiles
