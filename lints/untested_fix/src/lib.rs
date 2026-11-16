use std::path::Path;

use lint_pass_item::descendants;
#[cfg(feature = "plugin")]
use stiles_rust::ReadsTestCode as _;
use stiles_rust::{RuleId, RustLintPass, is_test_code};
use stiles_types::{DecoratedNode, Diagnostic, RuleOptions, Severity};

/// The id that every diagnostic of this rule carries
///
/// A project names the rule by this id when it configures the rule.
///
/// # Examples
///
/// ```
/// assert_eq!(untested_fix::RULE_ID.as_str(), "lint.untested-fix");
/// ```
pub const RULE_ID: RuleId = RuleId::new("lint.untested-fix");

/// The option that names the functions whose call in a test applies a fix
const FIX_ASSERTIONS: &str = "fix-assertions";

/// The function that applies a fix when the project sets no option
const DEFAULT_FIX_ASSERTION: &str = "assert_fix";

/// The method of `Diagnostic` that attaches a fix
const ATTACH_FIX: &str = "with_fix";

/// The name of the file that is the root of a library crate
const CRATE_ROOT: &str = "lib.rs";

/// Flags a fix in a crate root whose tests apply no fix
///
/// A fix rewrites the code of a project, and a broken fix breaks that code.
/// A test that applies the fix shows that it works. So each call to
/// `with_fix` outside test code needs a test that applies a fix. The rule
/// does not read a `with_fix` call inside a macro argument.
///
/// A test applies a fix when it calls a function that `fix-assertions`
/// names. The default is `assert_fix`, from `stiles_testing`. The rule
/// compares only the last segment of the called path or method. Such a call
/// inside a macro argument counts too. A project sets the option to replace the
/// default, so a project that also uses `assert_example` names both.
///
/// Stiles shows a rule one file at a time, so the proof must sit in the
/// file of the fix. The rule reads only a crate root named `lib.rs`, so a
/// module that its parent tests is skipped. It also skips a root that
/// declares a module in another file, because the tests may sit there. A
/// root without test code is skipped too. The one case the rule cannot see
/// is a crate that applies its fixes only in its `tests` directory. It
/// reports that crate, so such a crate applies one fix in `lib.rs` as well.
///
/// The Rust provider marks test code with the `TestCode` decoration. A file
/// is test code too when the kinds of its `Target` decoration are only
/// integration tests and benches. A `with_fix` call in test code is not
/// reported.
///
/// # Examples
///
/// ```text
/// // DON'T
/// impl RustLintPass for TodoComment {
///     fn check_line_comment(&mut self, node: &DecoratedNode<'_>) -> Vec<Diagnostic> {
///         finding(node).with_fix(removal(node)).into_iter().collect()
///     }
/// }
///
/// #[cfg(test)]
/// mod tests {
///     #[test]
///     fn todo_comment_is_flagged() {
///         assert_eq!(run("// TODO").len(), 1);
///     }
/// }
///
/// // DO
/// impl RustLintPass for TodoComment {
///     fn check_line_comment(&mut self, node: &DecoratedNode<'_>) -> Vec<Diagnostic> {
///         finding(node).with_fix(removal(node)).into_iter().collect()
///     }
/// }
///
/// #[cfg(test)]
/// mod tests {
///     #[test]
///     fn todo_comment_is_removed() {
///         assert_fix().tree(parse("// TODO\n", Language::Rust)).passes(passes).produces("");
///     }
/// }
/// ```
///
/// The rule reads the test code that the Rust provider marks. This example
/// marks the test by hand, in a tree parsed as `src/lib.rs`.
///
/// ```
/// use stiles_rust::RustLintPassAdapter;
/// use stiles_rust::decorations::TestCode;
/// use stiles_testing::{execute, parse_at};
/// use stiles_types::{Language, LintPass};
/// use untested_fix::UntestedFix;
///
/// let source = "fn f() { d.with_fix(fix); }\n#[test]\nfn t() { run(); }\n";
/// let mut tree = parse_at("src/lib.rs", source, Language::Rust);
/// let test = tree.root_node().named_child(2).expect("the source should hold a test").id();
/// tree.decorations_mut().insert(test, TestCode::Marked);
/// let pass: Box<dyn LintPass> = Box::new(RustLintPassAdapter::new(UntestedFix::default()));
///
/// let diagnostics = execute(&tree, &mut [pass]);
///
/// assert_eq!(diagnostics.len(), 1);
/// ```
pub struct UntestedFix {
    /// The names of the functions whose call in a test applies a fix
    fix_assertions: Vec<String>,
}

impl Default for UntestedFix {
    fn default() -> Self {
        Self {
            fix_assertions: default_fix_assertions(),
        }
    }
}

impl RustLintPass for UntestedFix {
    fn configure(&mut self, options: &RuleOptions) {
        self.fix_assertions = options
            .names(RULE_ID, FIX_ASSERTIONS)
            .unwrap_or_else(default_fix_assertions);
    }

    fn check_source_file(&mut self, node: &DecoratedNode<'_>) -> Vec<Diagnostic> {
        check_file(node, &self.fix_assertions)
    }
}

/// Returns the names that apply a fix when the project sets no option
fn default_fix_assertions() -> Vec<String> {
    vec![DEFAULT_FIX_ASSERTION.to_owned()]
}

/// Reports each fix in one crate root whose tests apply none
///
/// The rule fires on the absence of a test, so no single node carries the
/// answer. `source_file` is the one node that sees the whole file.
fn check_file(root: &DecoratedNode<'_>, fix_assertions: &[String]) -> Vec<Diagnostic> {
    if !is_crate_root(root.span().file()) || is_test_code(root) {
        return Vec::new();
    }
    if declares_module_elsewhere(root) || !holds_a_test(root) {
        return Vec::new();
    }
    if applies_a_fix(root, fix_assertions) {
        return Vec::new();
    }

    descendants(*root, "call_expression")
        .into_iter()
        .filter(|call| !is_test_code(call))
        .filter_map(|call| callee_name(&call))
        .filter(|name| name.text() == ATTACH_FIX)
        .map(|name| finding(&name, fix_assertions))
        .collect()
}

/// Returns whether a file is named as the root of a library crate
fn is_crate_root(file: &Path) -> bool {
    file.file_name()
        .is_some_and(|name| name.to_str() == Some(CRATE_ROOT))
}

/// Returns whether a file declares a module whose body sits in another file
fn declares_module_elsewhere(root: &DecoratedNode<'_>) -> bool {
    descendants(*root, "mod_item")
        .iter()
        .any(|module| module.child_by_field_name("body").is_none())
}

/// Returns whether a file holds a function that is test code
fn holds_a_test(root: &DecoratedNode<'_>) -> bool {
    descendants(*root, "function_item").iter().any(is_test_code)
}

/// Returns whether test code in a file calls a function that applies a fix
///
/// A call is a call expression, or a name before a parenthesized group in
/// the tokens of a macro call.
fn applies_a_fix(root: &DecoratedNode<'_>, fix_assertions: &[String]) -> bool {
    let named = |name: &DecoratedNode<'_>| fix_assertions.iter().any(|known| known == name.text());

    let in_calls = descendants(*root, "call_expression")
        .iter()
        .filter(|call| is_test_code(call))
        .filter_map(callee_name)
        .any(|name| named(&name));
    let in_macros = descendants(*root, "token_tree")
        .iter()
        .filter(|tokens| is_test_code(tokens))
        .flat_map(called_tokens)
        .any(|name| named(&name));
    in_calls || in_macros
}

/// Returns the node that names the function or method a call calls
///
/// The name is the last segment of a path, the field of a method call, or
/// either of those under a turbofish.
fn callee_name<'a>(call: &DecoratedNode<'a>) -> Option<DecoratedNode<'a>> {
    let mut function = call.child_by_field_name("function")?;
    if function.kind() == "generic_function" {
        function = function.child_by_field_name("function")?;
    }
    match function.kind() {
        "identifier" => Some(function),
        "scoped_identifier" => function.child_by_field_name("name"),
        "field_expression" => function.child_by_field_name("field"),
        _ => None,
    }
}

/// Returns the names in a token tree that a parenthesized group follows
fn called_tokens<'a>(tokens: &DecoratedNode<'a>) -> Vec<DecoratedNode<'a>> {
    tokens
        .named_children()
        .windows(2)
        .filter(|pair| pair[0].kind() == "identifier")
        .filter(|pair| pair[1].kind() == "token_tree" && pair[1].text().starts_with('('))
        .map(|pair| pair[0])
        .collect()
}

/// Builds the finding for one call to `with_fix`
fn finding(name: &DecoratedNode<'_>, fix_assertions: &[String]) -> Diagnostic {
    Diagnostic::builder()
        .rule_id(RULE_ID)
        .severity(Severity::Error)
        .message(format!(
            "`with_fix` attaches a fix, but no test in this file applies one; call {} in a test here",
            listed(fix_assertions)
        ))
        .span(name.span())
        .build()
}

/// Returns the names in backticks, joined as a sentence would
///
/// An empty option names no function, so the text points at the option.
fn listed(names: &[String]) -> String {
    let quoted: Vec<String> = names.iter().map(|name| format!("`{name}`")).collect();
    match quoted.as_slice() {
        [] => format!("a function that `{FIX_ASSERTIONS}` names"),
        [only] => only.clone(),
        [rest @ .., last] => format!("{} or {last}", rest.join(", ")),
    }
}

#[cfg(feature = "plugin")]
impl stiles_plugin::DeclaresRules for UntestedFix {
    fn rules(&self) -> Vec<RuleId> {
        vec![RULE_ID]
    }

    fn reads(&self) -> stiles_plugin::Reads {
        stiles_plugin::Reads::none().and_test_code()
    }
}

#[cfg(feature = "plugin")]
stiles_plugin::export_lints![rust: [UntestedFix::default()]];

#[cfg(test)]
mod tests {
    use stiles_rust::RustLintPassAdapter;
    use stiles_rust::decorations::TestCode;
    use stiles_testing::{assert_diagnostic, assert_no_diagnostics, execute};
    use stiles_types::{DecoratedTree, Language, LintPass, RuleOption};

    use super::*;

    /// A pass that attaches a fix, outside test code
    const PASS: &str = "impl RustLintPass for A {\n    fn check(&mut self) -> Diagnostic {\n        finding().with_fix(fix())\n    }\n}\n";

    fn assert_auto_traits<T: Send + Sync + Unpin>() {}

    /// Parses `source` as the file at `path` and marks its test code
    ///
    /// The provider marks each item that a test attribute or a test `cfg`
    /// decorates. The syntactic check of `lint_pass_item` finds the same
    /// items here.
    fn parse_at(path: &Path, source: &str) -> DecoratedTree {
        let mut tree = stiles_testing::parse_at(path, source, Language::Rust);
        let marked: Vec<usize> = descendants(tree.root_node(), "function_item")
            .into_iter()
            .chain(descendants(tree.root_node(), "mod_item"))
            .filter(lint_pass_item::is_test_code)
            .map(|item| item.id())
            .collect();
        for id in marked {
            tree.decorations_mut().insert(id, TestCode::Marked);
        }
        tree
    }

    fn run(source: &str) -> Vec<Diagnostic> {
        run_at(Path::new("src/lib.rs"), source)
    }

    fn run_at(path: &Path, source: &str) -> Vec<Diagnostic> {
        let tree = parse_at(path, source);
        let pass: Box<dyn LintPass> = Box::new(RustLintPassAdapter::new(UntestedFix::default()));
        execute(&tree, &mut [pass])
    }

    fn run_with_fix_assertions(source: &str, names: &[&str]) -> Vec<Diagnostic> {
        let tree = parse_at(Path::new("src/lib.rs"), source);
        let options = RuleOptions::new(vec![
            RuleOption::builder()
                .rule(RULE_ID.as_str().to_owned())
                .name(FIX_ASSERTIONS)
                .values(names.iter().map(|name| (*name).to_owned()).collect())
                .build(),
        ]);
        let mut pass = RustLintPassAdapter::new(UntestedFix::default());
        pass.configure(&options);
        let mut passes: Vec<Box<dyn LintPass>> = vec![Box::new(pass)];
        execute(&tree, &mut passes)
    }

    /// Returns `PASS` followed by a test module whose one test runs `body`
    fn tested_by(body: &str) -> String {
        format!(
            "{PASS}#[cfg(test)]\nmod tests {{\n    #[test]\n    fn t() {{\n        {body}\n    }}\n}}\n"
        )
    }

    #[test]
    fn assertion_in_a_macro_argument_is_not_flagged() {
        let diagnostics = run(&tested_by("check!(assert_fix().produces(\"\"));"));

        assert_no_diagnostics(&diagnostics);
    }

    #[test]
    fn assertion_in_shipped_code_is_flagged() {
        let source = format!("fn helper() {{ assert_fix(); }}\n{}", tested_by("run();"));

        let diagnostics = run(&source);

        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn assertion_on_a_path_is_not_flagged() {
        let diagnostics = run(&tested_by("stiles_testing::assert_fix().produces(\"\");"));

        assert_no_diagnostics(&diagnostics);
    }

    #[test]
    fn assertion_under_a_turbofish_is_not_flagged() {
        let diagnostics = run(&tested_by("assert_fix::<A>();"));

        assert_no_diagnostics(&diagnostics);
    }

    #[test]
    fn configured_assertion_is_not_flagged() {
        let source = tested_by("pair.assert_example(run);");

        let diagnostics = run_with_fix_assertions(&source, &["assert_fix", "assert_example"]);

        assert_no_diagnostics(&diagnostics);
    }

    #[test]
    fn configured_assertions_replace_the_default() {
        let source = tested_by("assert_fix().produces(\"\");");

        let diagnostics = run_with_fix_assertions(&source, &["assert_example"]);

        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn documented_example_is_flagged() {
        let pair = example_pair::ExamplePair::read(include_str!("lib.rs"));

        let diagnostics = run(pair.example());

        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn documented_fix_is_not_flagged() {
        let pair = example_pair::ExamplePair::read(include_str!("lib.rs"));

        let diagnostics = run(pair.fix());

        assert_no_diagnostics(&diagnostics);
    }

    #[test]
    fn empty_option_names_the_option_in_the_message() {
        let source = tested_by("assert_fix().produces(\"\");");

        let diagnostics = run_with_fix_assertions(&source, &[]);

        assert_diagnostic(&diagnostics[0])
            .message_contains("a function that `fix-assertions` names");
    }

    #[test]
    fn file_other_than_the_crate_root_is_not_flagged() {
        let diagnostics = run_at(Path::new("src/fix.rs"), &tested_by("run();"));

        assert_no_diagnostics(&diagnostics);
    }

    #[test]
    fn file_without_test_code_is_not_flagged() {
        let diagnostics = run(PASS);

        assert_no_diagnostics(&diagnostics);
    }

    #[test]
    fn fix_in_a_path_call_is_flagged() {
        let source = tested_by("run();").replace(
            "finding().with_fix(fix())",
            "Diagnostic::with_fix(finding(), fix())",
        );

        let diagnostics = run(&source);

        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn fix_in_test_code_is_not_flagged() {
        let source = "#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {\n        finding().with_fix(fix());\n    }\n}\n";

        let diagnostics = run(source);

        assert_no_diagnostics(&diagnostics);
    }

    #[test]
    fn imported_assertion_without_a_call_is_flagged() {
        let source = tested_by("run();").replace(
            "mod tests {\n",
            "mod tests {\n    use stiles_testing::assert_fix;\n",
        );

        let diagnostics = run(&source);

        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn message_names_every_configured_assertion() {
        let source = tested_by("run();");

        let diagnostics = run_with_fix_assertions(&source, &["assert_fix", "assert_example"]);

        assert_diagnostic(&diagnostics[0]).message_contains("`assert_fix` or `assert_example`");
    }

    #[test]
    fn module_in_another_file_is_not_flagged() {
        let source = format!("mod fix;\n{}", tested_by("run();"));

        let diagnostics = run(&source);

        assert_no_diagnostics(&diagnostics);
    }

    #[test]
    fn pass_is_send_sync_and_unpin() {
        assert_auto_traits::<UntestedFix>();
    }

    #[cfg(feature = "plugin")]
    #[test]
    fn reads_returns_the_target_and_the_test_code() {
        let reads = stiles_plugin::DeclaresRules::reads(&UntestedFix::default());

        assert_eq!(
            reads,
            stiles_plugin::Reads::of::<stiles_rust::decorations::Target>()
                .and::<stiles_rust::decorations::TestCode>()
        );
    }

    #[test]
    fn test_module_in_another_file_is_not_flagged() {
        let source = format!("{PASS}#[cfg(test)]\nmod tests;\n#[test]\nfn t() {{ run(); }}\n");

        let diagnostics = run(&source);

        assert_no_diagnostics(&diagnostics);
    }

    #[test]
    fn untested_fix_is_flagged_on_the_method_name() {
        let diagnostics = run(&tested_by("run();"));

        assert_diagnostic(&diagnostics[0])
            .has_rule_id("lint.untested-fix")
            .has_severity(Severity::Error)
            .has_span("src/lib.rs", 84..92);
    }
}
