use lint_pass_item::{
    descendants, doc_lines, implementing_type, lint_pass_impls, string_constants, string_value,
    struct_named,
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
/// assert_eq!(undocumented_option::RULE_ID.as_str(), "lint.undocumented-option");
/// ```
pub const RULE_ID: RuleId = RuleId::new("lint.undocumented-option");

/// The `RuleOptions` method that reads an option by name
const OPTION_READER: &str = "names";

/// Flags a rule option whose name the docs of its pass do not mention
///
/// A project can only set an option that it knows exists. A rule reads an
/// option through `options.names(RULE_ID, "...")`, and nothing else tells a
/// reader the name. So the docs of the pass must name each option it
/// reads, in a code span such as `` `boundary-attributes` ``.
///
/// The rule reads the `impl` blocks of a lint pass trait in one file. Each
/// call to `names` there with a known option name is checked against the
/// docs of the pass. The name is a string literal, or a constant in the
/// same file that holds one. A name the file does not spell out is skipped,
/// and so is a pass whose struct sits in another file.
///
/// # Examples
///
/// ```text
/// // DON'T
/// /// Flags `bool` parameters
/// pub struct BoolParam;
///
/// impl RustLintPass for BoolParam {
///     fn configure(&mut self, options: &RuleOptions) {
///         options.names(RULE_ID, "boundary-attributes");
///     }
/// }
///
/// // DO
/// /// Flags `bool` parameters
/// ///
/// /// `boundary-attributes` names attributes that mark a boundary.
/// pub struct BoolParam;
///
/// impl RustLintPass for BoolParam {
///     fn configure(&mut self, options: &RuleOptions) {
///         options.names(RULE_ID, "boundary-attributes");
///     }
/// }
/// ```
///
/// ```
/// use stiles_rust::RustLintPassAdapter;
/// use stiles_testing::{execute, parse};
/// use stiles_types::{Language, LintPass};
/// use undocumented_option::UndocumentedOption;
///
/// let source = "pub struct A;\nimpl RustLintPass for A {\n    fn configure(&mut self, o: &RuleOptions) { o.names(RULE_ID, \"depth\"); }\n}";
/// let tree = parse(source, Language::Rust);
/// let pass: Box<dyn LintPass> = Box::new(RustLintPassAdapter::new(UndocumentedOption));
///
/// let diagnostics = execute(&tree, &mut [pass]);
///
/// assert_eq!(diagnostics.len(), 1);
/// ```
pub struct UndocumentedOption;

/// Returns the option name argument of a call that reads an option
///
/// The call is a method call to `names` with two arguments, and the name
/// is the second.
fn option_argument<'a>(call: &DecoratedNode<'a>) -> Option<DecoratedNode<'a>> {
    let function = call.child_by_field_name("function")?;
    if function.kind() != "field_expression" {
        return None;
    }
    if function.child_by_field_name("field")?.text() != OPTION_READER {
        return None;
    }
    let arguments = call.child_by_field_name("arguments")?.named_children();
    match arguments.as_slice() {
        [_, name] => Some(*name),
        _ => None,
    }
}

impl RustLintPass for UndocumentedOption {
    fn check_source_file(&mut self, node: &DecoratedNode<'_>) -> Vec<Diagnostic> {
        let constants = string_constants(*node);
        let mut diagnostics = Vec::new();

        for block in lint_pass_impls(*node) {
            let Some(pass) = implementing_type(&block) else {
                continue;
            };
            let Some(item) = struct_named(*node, pass) else {
                continue;
            };
            let docs = doc_lines(&item).join("\n");

            for call in descendants(block, "call_expression") {
                let Some(argument) = option_argument(&call) else {
                    continue;
                };
                let name = match argument.kind() {
                    "identifier" => constants.get(argument.text()).copied(),
                    _ => string_value(&argument),
                };
                let Some(name) = name else {
                    continue;
                };
                if docs.contains(&format!("`{name}`")) {
                    continue;
                }
                diagnostics.push(
                    Diagnostic::builder()
                        .rule_id(RULE_ID)
                        .severity(Severity::Error)
                        .message(format!(
                            "`{pass}` reads the option `{name}`, which its docs do not name; \
                             describe it there in a code span"
                        ))
                        .span(argument.span())
                        .build(),
                );
            }
        }
        diagnostics
    }
}

#[cfg(feature = "plugin")]
impl stiles_plugin::DeclaresRules for UndocumentedOption {
    fn rules(&self) -> Vec<RuleId> {
        vec![RULE_ID]
    }

    fn reads(&self) -> stiles_plugin::Reads {
        stiles_plugin::Reads::none()
    }
}

#[cfg(feature = "plugin")]
stiles_plugin::export_lints![rust: [UndocumentedOption]];

#[cfg(test)]
mod tests {
    use stiles_rust::RustLintPassAdapter;
    use stiles_testing::{assert_diagnostic, assert_no_diagnostics, execute, parse};
    use stiles_types::{Language, LintPass};

    use super::*;

    fn assert_auto_traits<T: Send + Sync + Unpin>() {}

    fn run(source: &str) -> Vec<Diagnostic> {
        let tree = parse(source, Language::Rust);
        let pass: Box<dyn LintPass> = Box::new(RustLintPassAdapter::new(UndocumentedOption));
        execute(&tree, &mut [pass])
    }

    fn pass_reading(argument: &str) -> String {
        format!(
            "pub struct A;\nimpl RustLintPass for A {{\n    fn configure(&mut self, o: &RuleOptions) {{\n        o.names(RULE_ID, {argument});\n    }}\n}}\n"
        )
    }

    #[test]
    fn constant_from_another_crate_is_not_flagged() {
        let diagnostics = run(&pass_reading("boundary::OPTION"));

        assert_no_diagnostics(&diagnostics);
    }

    #[test]
    fn constant_in_the_file_is_flagged_when_undocumented() {
        let source = format!(
            "const OPTION: &str = \"depth\";\n{}",
            pass_reading("OPTION")
        );

        let diagnostics = run(&source);

        assert_diagnostic(&diagnostics[0]).message_contains("`depth`");
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
    fn name_documented_in_a_code_span_is_not_flagged() {
        let diagnostics = run(&format!("/// Reads `depth`\n{}", pass_reading("\"depth\"")));

        assert_no_diagnostics(&diagnostics);
    }

    #[test]
    fn name_in_plain_prose_is_flagged() {
        let diagnostics = run(&format!("/// Reads depth\n{}", pass_reading("\"depth\"")));

        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn names_call_outside_a_lint_pass_impl_is_not_flagged() {
        let diagnostics = run(
            "pub struct A;\nimpl A {\n    fn f(o: &RuleOptions) { o.names(RULE_ID, \"depth\"); }\n}",
        );

        assert_no_diagnostics(&diagnostics);
    }

    #[test]
    fn pass_defined_in_another_file_is_not_flagged() {
        let diagnostics = run(
            "impl RustLintPass for A {\n    fn configure(&mut self, o: &RuleOptions) { o.names(RULE_ID, \"depth\"); }\n}",
        );

        assert_no_diagnostics(&diagnostics);
    }

    #[test]
    fn pass_is_send_sync_and_unpin() {
        assert_auto_traits::<UndocumentedOption>();
    }

    #[test]
    fn undocumented_literal_is_flagged_on_the_name() {
        let source = pass_reading("\"depth\"");

        let diagnostics = run(&source);

        assert_diagnostic(&diagnostics[0])
            .has_rule_id("lint.undocumented-option")
            .has_severity(Severity::Error)
            .has_span("<test>", 112..119);
    }
}
