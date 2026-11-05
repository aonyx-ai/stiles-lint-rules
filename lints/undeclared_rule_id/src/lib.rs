use std::collections::{BTreeMap, BTreeSet};

use lint_pass_item::{
    descendants, implemented_trait, is_test_code, string_constants, string_value, wrapped_string,
};
use stiles_rust::{RuleId, RustLintPass};
use stiles_types::{DecoratedNode, Diagnostic, Severity};

/// The id that every diagnostic of this rule carries
///
/// A project names the rule by this id when it configures the rule.
///
/// # Examples
///
/// ```
/// assert_eq!(undeclared_rule_id::RULE_ID.as_str(), "lint.undeclared-rule-id");
/// ```
pub const RULE_ID: RuleId = RuleId::new("lint.undeclared-rule-id");

/// The trait whose implementation lists the rule ids a pass reports
const DECLARES_RULES: &str = "DeclaresRules";

/// The builder method that sets the rule id of a diagnostic
const RULE_ID_SETTER: &str = "rule_id";

/// Flags a diagnostic whose rule id the pass does not declare
///
/// Stiles refuses a diagnostic whose rule id the pass did not declare in
/// `DeclaresRules`. It finds out only at run time, on the file that
/// triggers the diagnostic. The author then learns about the mistake from
/// a failed check in someone else's project.
///
/// The rule compares, within one file, each id passed to `.rule_id(...)`
/// with the ids that the `DeclaresRules` implementations there return. An id
/// is a `RuleId::new("...")` call, or a constant in the same file that holds
/// one. An id the file does not spell out is skipped. So is a file without
/// `DeclaresRules`, and test code, which builds diagnostics of its own.
///
/// # Examples
///
/// ```text
/// // DON'T
/// const RULE_ID: RuleId = RuleId::new("lint.todo-comment");
///
/// fn report(span: Span) -> Diagnostic {
///     Diagnostic::builder().rule_id(RuleId::new("lint.todo")).span(span).build()
/// }
///
/// impl DeclaresRules for TodoComment {
///     fn rules(&self) -> Vec<RuleId> {
///         vec![RULE_ID]
///     }
/// }
///
/// // DO
/// const RULE_ID: RuleId = RuleId::new("lint.todo-comment");
///
/// fn report(span: Span) -> Diagnostic {
///     Diagnostic::builder().rule_id(RULE_ID).span(span).build()
/// }
///
/// impl DeclaresRules for TodoComment {
///     fn rules(&self) -> Vec<RuleId> {
///         vec![RULE_ID]
///     }
/// }
/// ```
///
/// ```
/// use stiles_rust::RustLintPassAdapter;
/// use stiles_testing::{execute, parse};
/// use stiles_types::{Language, LintPass};
/// use undeclared_rule_id::UndeclaredRuleId;
///
/// let source = "fn f() { d.rule_id(RuleId::new(\"lint.b\")); }\nimpl DeclaresRules for A {\n    fn rules(&self) -> Vec<RuleId> { vec![RuleId::new(\"lint.a\")] }\n}";
/// let tree = parse(source, Language::Rust);
/// let pass: Box<dyn LintPass> = Box::new(RustLintPassAdapter::new(UndeclaredRuleId));
///
/// let diagnostics = execute(&tree, &mut [pass]);
///
/// assert_eq!(diagnostics.len(), 1);
/// ```
pub struct UndeclaredRuleId;

/// Returns the rule id an expression names, when the file spells it out
fn named_id<'a>(
    expression: &DecoratedNode<'a>,
    constants: &BTreeMap<&'a str, &'a str>,
) -> Option<&'a str> {
    match expression.kind() {
        "identifier" => constants.get(expression.text()).copied(),
        "call_expression" => wrapped_string(expression),
        _ => None,
    }
}

/// Returns the rule id that the tokens of a macro argument name
///
/// Inside a macro such as `vec![...]`, `.rule_id(...)` is a run of tokens.
/// Its argument reads as a constant's name, or as `RuleId::new("...")`.
fn tokens_id<'a>(
    argument: &DecoratedNode<'a>,
    constants: &BTreeMap<&'a str, &'a str>,
) -> Option<&'a str> {
    let inner = argument.text().strip_prefix('(')?.strip_suffix(')')?.trim();
    if inner
        .chars()
        .all(|character| character.is_alphanumeric() || character == '_')
    {
        return constants.get(inner).copied();
    }
    let literal = inner
        .strip_prefix("RuleId::new(")?
        .strip_suffix(')')?
        .trim()
        .strip_prefix('"')?
        .strip_suffix('"')?;
    match literal.contains(['"', '\\']) {
        true => None,
        false => Some(literal),
    }
}

/// Returns the argument token trees of `.rule_id(...)` runs inside macros
fn token_arguments(root: DecoratedNode<'_>) -> Vec<DecoratedNode<'_>> {
    let mut arguments = Vec::new();
    for tree in descendants(root, "token_tree") {
        let children: Vec<DecoratedNode<'_>> = (0..tree.child_count())
            .filter_map(|index| u32::try_from(index).ok())
            .filter_map(|index| tree.child(index))
            .collect();
        for window in children.windows(3) {
            let [dot, setter, argument] = window else {
                continue;
            };
            if dot.text() == "."
                && setter.text() == RULE_ID_SETTER
                && argument.kind() == "token_tree"
            {
                arguments.push(*argument);
            }
        }
    }
    arguments
}

/// Returns every rule id that the `DeclaresRules` implementations return
///
/// Each string literal in such an implementation counts as an id, and so
/// does each constant it names. A `vec![...]` holds its contents as tokens
/// rather than expressions, so the literals are read wherever they sit.
fn declared_ids<'a>(
    root: DecoratedNode<'a>,
    constants: &BTreeMap<&'a str, &'a str>,
) -> Option<BTreeSet<&'a str>> {
    let blocks: Vec<DecoratedNode<'a>> = descendants(root, "impl_item")
        .into_iter()
        .filter(|block| implemented_trait(block) == Some(DECLARES_RULES))
        .filter(|block| !is_test_code(block))
        .collect();
    if blocks.is_empty() {
        return None;
    }

    let mut ids = BTreeSet::new();
    for block in blocks {
        ids.extend(
            descendants(block, "identifier")
                .iter()
                .filter_map(|identifier| constants.get(identifier.text()).copied()),
        );
        ids.extend(
            descendants(block, "string_literal")
                .iter()
                .filter_map(string_value),
        );
    }
    Some(ids)
}

/// Returns the argument of a `.rule_id(...)` call
fn rule_id_argument<'a>(call: &DecoratedNode<'a>) -> Option<DecoratedNode<'a>> {
    let function = call.child_by_field_name("function")?;
    if function.kind() != "field_expression" {
        return None;
    }
    if function.child_by_field_name("field")?.text() != RULE_ID_SETTER {
        return None;
    }
    let arguments = call.child_by_field_name("arguments")?.named_children();
    match arguments.as_slice() {
        [only] => Some(*only),
        _ => None,
    }
}

impl RustLintPass for UndeclaredRuleId {
    fn check_source_file(&mut self, node: &DecoratedNode<'_>) -> Vec<Diagnostic> {
        let constants = string_constants(*node);
        let Some(declared) = declared_ids(*node, &constants) else {
            return Vec::new();
        };

        let calls = descendants(*node, "call_expression")
            .into_iter()
            .filter_map(|call| rule_id_argument(&call))
            .filter_map(|argument| Some((argument, named_id(&argument, &constants)?)));
        let tokens = token_arguments(*node)
            .into_iter()
            .filter_map(|argument| Some((argument, tokens_id(&argument, &constants)?)));

        calls
            .chain(tokens)
            .filter(|(argument, id)| !declared.contains(id) && !is_test_code(argument))
            .map(|(argument, id)| {
                Diagnostic::builder()
                    .rule_id(RULE_ID)
                    .severity(Severity::Error)
                    .message(format!(
                        "this diagnostic reports `{id}`, which no `DeclaresRules` in this \
                         file declares; stiles refuses it at run time"
                    ))
                    .span(argument.span())
                    .build()
            })
            .collect()
    }
}

#[cfg(feature = "plugin")]
impl stiles_plugin::DeclaresRules for UndeclaredRuleId {
    fn rules(&self) -> Vec<RuleId> {
        vec![RULE_ID]
    }
}

#[cfg(feature = "plugin")]
stiles_plugin::export_lints![rust: [UndeclaredRuleId]];

#[cfg(test)]
mod tests {
    use stiles_rust::RustLintPassAdapter;
    use stiles_testing::{assert_diagnostic, assert_no_diagnostics, execute, parse};
    use stiles_types::{Language, LintPass};

    use super::*;

    const DECLARES_A: &str = "impl DeclaresRules for A {\n    fn rules(&self) -> Vec<RuleId> {\n        vec![RuleId::new(\"lint.a\")]\n    }\n}\n";

    fn assert_auto_traits<T: Send + Sync + Unpin>() {}

    fn run(source: &str) -> Vec<Diagnostic> {
        let tree = parse(source, Language::Rust);
        let pass: Box<dyn LintPass> = Box::new(RustLintPassAdapter::new(UndeclaredRuleId));
        execute(&tree, &mut [pass])
    }

    fn reporting(argument: &str) -> String {
        format!("fn report(d: Builder) {{\n    d.rule_id({argument});\n}}\n{DECLARES_A}")
    }

    #[test]
    fn declared_literal_is_not_flagged() {
        let diagnostics = run(&reporting("RuleId::new(\"lint.a\")"));

        assert_no_diagnostics(&diagnostics);
    }

    #[test]
    fn diagnostic_built_inside_a_macro_is_flagged() {
        let source = format!(
            "fn report() -> Vec<Diagnostic> {{\n    vec![Diagnostic::builder().rule_id(RuleId::new(\"lint.b\")).build()]\n}}\n{DECLARES_A}"
        );

        let diagnostics = run(&source);

        assert_diagnostic(&diagnostics[0]).message_contains("`lint.b`");
    }

    #[test]
    fn documented_example_is_flagged() {
        let pair = example_pair::ExamplePair::read(include_str!("lib.rs"));

        let diagnostics = run(pair.example());

        assert!(!diagnostics.is_empty());
    }

    #[test]
    fn documented_fix_is_not_flagged() {
        let pair = example_pair::ExamplePair::read(include_str!("lib.rs"));

        let diagnostics = run(pair.fix());

        assert_no_diagnostics(&diagnostics);
    }

    #[test]
    fn file_without_declares_rules_is_not_flagged() {
        let diagnostics =
            run("fn report(d: Builder) {\n    d.rule_id(RuleId::new(\"lint.b\"));\n}\n");

        assert_no_diagnostics(&diagnostics);
    }

    #[test]
    fn id_from_another_crate_is_not_flagged() {
        let diagnostics = run(&reporting("other::RULE_ID"));

        assert_no_diagnostics(&diagnostics);
    }

    #[test]
    fn pass_is_send_sync_and_unpin() {
        assert_auto_traits::<UndeclaredRuleId>();
    }

    #[test]
    fn test_code_is_not_flagged() {
        let source = format!(
            "{DECLARES_A}#[cfg(test)]\nmod tests {{\n    fn f(d: Builder) {{ d.rule_id(RuleId::new(\"test.rule\")); }}\n}}\n"
        );

        let diagnostics = run(&source);

        assert_no_diagnostics(&diagnostics);
    }

    #[test]
    fn undeclared_constant_is_flagged() {
        let source = format!(
            "const OTHER: RuleId = RuleId::new(\"lint.b\");\n{}",
            reporting("OTHER")
        );

        let diagnostics = run(&source);

        assert_diagnostic(&diagnostics[0]).message_contains("`lint.b`");
    }

    #[test]
    fn undeclared_literal_is_flagged_on_the_argument() {
        let source = reporting("RuleId::new(\"lint.b\")");

        let diagnostics = run(&source);

        assert_diagnostic(&diagnostics[0])
            .has_rule_id("lint.undeclared-rule-id")
            .has_severity(Severity::Error)
            .has_span("<test>", 38..59);
    }
}
