use std::collections::BTreeSet;

use lint_pass_item::{doc_lines, implementing_type, lint_pass_impls, struct_named};
use stiles_rust::{RuleId, RustLintPass};
use stiles_types::{DecoratedNode, Diagnostic, Severity};

/// The id that every diagnostic of this rule carries
///
/// A project names the rule by this id when it configures the rule.
///
/// # Examples
///
/// ```
/// assert_eq!(incomplete_pass_doc::RULE_ID.as_str(), "lint.incomplete-pass-doc");
/// ```
pub const RULE_ID: RuleId = RuleId::new("lint.incomplete-pass-doc");

/// The heading that opens the examples of a pass
const EXAMPLES_HEADING: &str = "# Examples";

/// The marker line that opens a negative example
const EXAMPLE_MARKER: &str = "// DON'T";

/// The marker line that opens the fix of a negative example
const FIX_MARKER: &str = "// DO";

/// Flags a public lint pass whose docs cannot describe its rule
///
/// A reference page of rules shows the docs of each pass. A reader there
/// chooses rules and looks up diagnostics, so each entry needs four parts.
/// The doc comment opens with a summary. A body below the summary says what
/// the rule reports and why. An `# Examples` section holds a code block
/// with a negative example under a `// DON'T` line, and its fix under a
/// later `// DO` line. The rule reports the first part that is missing.
///
/// A pass is a struct that implements a lint pass trait, such as
/// `RustLintPass` or `TypeScriptLintPass`, in the same file. A private
/// struct and a pass in test code are skipped, because no reference page
/// shows them. The markers make the check syntactic: the rule finds them in
/// a fence, and it never runs the pass on the example.
///
/// # Examples
///
/// ```text
/// // DON'T
/// /// Flags `bool` parameters
/// pub struct BoolParam;
///
/// impl RustLintPass for BoolParam {}
///
/// // DO
/// /// Flags `bool` parameters
/// ///
/// /// A `bool` hides its meaning at the call site.
/// ///
/// /// # Examples
/// ///
/// /// ~~~text
/// /// // DON'T
/// /// fn toggle(enabled: bool) {}
/// ///
/// /// // DO
/// /// fn toggle(state: State) {}
/// /// ~~~
/// pub struct BoolParam;
///
/// impl RustLintPass for BoolParam {}
/// ```
///
/// ```
/// use incomplete_pass_doc::IncompletePassDoc;
/// use stiles_rust::RustLintPassAdapter;
/// use stiles_testing::{execute, parse};
/// use stiles_types::{Language, LintPass};
///
/// let source = "/// Flags a thing\npub struct Todo;\nimpl RustLintPass for Todo {}";
/// let tree = parse(source, Language::Rust);
/// let pass: Box<dyn LintPass> = Box::new(RustLintPassAdapter::new(IncompletePassDoc));
///
/// let diagnostics = execute(&tree, &mut [pass]);
///
/// assert_eq!(diagnostics.len(), 1);
/// ```
pub struct IncompletePassDoc;

/// The first part that the docs of a pass lack
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum MissingPart {
    /// The pass has no doc comment at all
    Docs,
    /// The docs stop at the summary
    Body,
    /// The docs have no `# Examples` heading
    Examples,
    /// The examples hold no `// DON'T` example with a later `// DO` fix
    Pair,
}

impl MissingPart {
    /// Returns the message that names the part and what it is for
    fn message(self, pass: &str) -> String {
        match self {
            Self::Docs => format!(
                "lint pass `{pass}` has no doc comment; document what its rule reports and why"
            ),
            Self::Body => format!(
                "the docs of lint pass `{pass}` stop at the summary; add a body that says what \
                 its rule reports and why"
            ),
            Self::Examples => {
                format!("the docs of lint pass `{pass}` have no `{EXAMPLES_HEADING}` section")
            }
            Self::Pair => format!(
                "the examples of lint pass `{pass}` hold no `{EXAMPLE_MARKER}` example with its \
                 `{FIX_MARKER}` fix"
            ),
        }
    }
}

/// Where a doc line sits against the code fences around it
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum Fence {
    /// The line opens, closes, or sits inside a code fence
    Inside,
    /// The line is prose or a heading
    Outside,
}

/// One line of a doc comment, read against the fences around it
struct Line<'a> {
    text: &'a str,
    fence: Fence,
}

/// Returns each doc line with whether it sits inside a code fence
///
/// A fence opens with three backticks or three tildes, and only the same
/// character closes it. The fence lines themselves count as fenced.
fn classify(docs: &[String]) -> Vec<Line<'_>> {
    let mut open: Option<char> = None;
    docs.iter()
        .map(|text| {
            let trimmed = text.trim_start();
            let marker = ['`', '~']
                .into_iter()
                .find(|character| trimmed.starts_with(&character.to_string().repeat(3)));
            let fence = match open.is_some() || marker.is_some() {
                true => Fence::Inside,
                false => Fence::Outside,
            };
            match (open, marker) {
                (None, Some(character)) => open = Some(character),
                (Some(current), Some(character)) if current == character => open = None,
                _ => {}
            }
            Line {
                text: text.trim(),
                fence,
            }
        })
        .collect()
}

/// Returns whether the docs hold prose below their summary paragraph
fn has_body(lines: &[Line<'_>]) -> bool {
    let summary_end = lines
        .iter()
        .position(|line| line.text.is_empty())
        .unwrap_or(lines.len());
    lines[summary_end..].iter().any(|Line { text, fence }| {
        *fence == Fence::Outside && !text.is_empty() && !text.starts_with('#')
    })
}

/// Returns whether a fenced block in the lines holds the example and its fix
fn has_pair(section: &[Line<'_>]) -> bool {
    let mut example_seen = false;
    for line in section {
        match line.fence {
            Fence::Outside => {
                example_seen = false;
                continue;
            }
            Fence::Inside => {}
        }
        if line.text == EXAMPLE_MARKER {
            example_seen = true;
        } else if line.text == FIX_MARKER && example_seen {
            return true;
        }
    }
    false
}

/// Returns the first part that the docs of a pass lack
fn missing_part(docs: &[String]) -> Option<MissingPart> {
    if docs.iter().all(|line| line.trim().is_empty()) {
        return Some(MissingPart::Docs);
    }
    let lines = classify(docs);
    if !has_body(&lines) {
        return Some(MissingPart::Body);
    }
    let Some(start) = lines
        .iter()
        .position(|line| line.fence == Fence::Outside && line.text == EXAMPLES_HEADING)
    else {
        return Some(MissingPart::Examples);
    };
    let section = &lines[start + 1..];
    let end = section
        .iter()
        .position(|line| line.fence == Fence::Outside && line.text.starts_with("# "))
        .unwrap_or(section.len());
    match has_pair(&section[..end]) {
        true => None,
        false => Some(MissingPart::Pair),
    }
}

/// Returns whether an item is public outside its crate
fn is_public(item: &DecoratedNode<'_>) -> bool {
    item.named_children()
        .iter()
        .any(|child| child.kind() == "visibility_modifier" && child.text() == "pub")
}

impl RustLintPass for IncompletePassDoc {
    fn check_source_file(&mut self, node: &DecoratedNode<'_>) -> Vec<Diagnostic> {
        let passes: BTreeSet<&str> = lint_pass_impls(*node)
            .iter()
            .filter_map(implementing_type)
            .collect();

        passes
            .into_iter()
            .filter_map(|pass| {
                let item = struct_named(*node, pass)?;
                if !is_public(&item) {
                    return None;
                }
                let missing = missing_part(&doc_lines(&item))?;
                let name = item.child_by_field_name("name")?;
                Some(
                    Diagnostic::builder()
                        .rule_id(RULE_ID)
                        .severity(Severity::Error)
                        .message(missing.message(pass))
                        .span(name.span())
                        .build(),
                )
            })
            .collect()
    }
}

#[cfg(feature = "plugin")]
impl stiles_plugin::DeclaresRules for IncompletePassDoc {
    fn rules(&self) -> Vec<RuleId> {
        vec![RULE_ID]
    }

    fn reads(&self) -> stiles_plugin::Reads {
        stiles_plugin::Reads::none()
    }
}

#[cfg(feature = "plugin")]
stiles_plugin::export_lints![rust: [IncompletePassDoc]];

#[cfg(test)]
mod tests {
    use stiles_rust::RustLintPassAdapter;
    use stiles_testing::{assert_diagnostic, assert_no_diagnostics, execute, parse};
    use stiles_types::{Language, LintPass};

    use super::*;

    const COMPLETE: &str = "/// Flags a thing\n///\n/// A thing hides intent.\n///\n/// # Examples\n///\n/// ```text\n/// // DON'T\n/// thing();\n///\n/// // DO\n/// other();\n/// ```\npub struct Todo;\n\nimpl RustLintPass for Todo {}\n";

    fn run(source: &str) -> Vec<Diagnostic> {
        let tree = parse(source, Language::Rust);
        let pass: Box<dyn LintPass> = Box::new(RustLintPassAdapter::new(IncompletePassDoc));
        execute(&tree, &mut [pass])
    }

    fn assert_auto_traits<T: Send + Sync + Unpin>() {}

    fn missing(docs: &str) -> Option<MissingPart> {
        let lines: Vec<String> = docs.lines().map(str::to_string).collect();
        missing_part(&lines)
    }

    #[test]
    fn complete_docs_are_not_flagged() {
        let diagnostics = run(COMPLETE);

        assert_no_diagnostics(&diagnostics);
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
    fn fence_is_send_sync_and_unpin() {
        assert_auto_traits::<Fence>();
    }

    #[test]
    fn heading_inside_a_fence_does_not_open_the_examples() {
        let found = missing("Flags a thing\n\nBody\n\n```\n# Examples\n// DON'T\n// DO\n```");

        assert_eq!(found, Some(MissingPart::Examples));
    }

    #[test]
    fn line_is_send_sync_and_unpin() {
        assert_auto_traits::<Line<'static>>();
    }

    #[test]
    fn markers_in_two_separate_fences_are_not_a_pair() {
        let found = missing(
            "Flags a thing\n\nBody\n\n# Examples\n\n```text\n// DON'T\n```\n\n```text\n// DO\n```",
        );

        assert_eq!(found, Some(MissingPart::Pair));
    }

    #[test]
    fn markers_outside_a_fence_are_not_a_pair() {
        let found = missing("Flags a thing\n\nBody\n\n# Examples\n\n// DON'T\n\n// DO");

        assert_eq!(found, Some(MissingPart::Pair));
    }

    #[test]
    fn markers_under_a_later_section_are_not_examples() {
        let found = missing(
            "Flags a thing\n\nBody\n\n# Examples\n\nNone.\n\n# Panics\n\n```text\n// DON'T\n// DO\n```",
        );

        assert_eq!(found, Some(MissingPart::Pair));
    }

    #[test]
    fn missing_part_is_send_sync_and_unpin() {
        assert_auto_traits::<MissingPart>();
    }

    #[test]
    fn pair_in_a_tilde_fence_is_found() {
        let found =
            missing("Flags a thing\n\nBody\n\n# Examples\n\n~~~text\n// DON'T\na\n// DO\nb\n~~~");

        assert_eq!(found, None);
    }

    #[test]
    fn pass_implementing_two_traits_is_flagged_once() {
        let diagnostics = run(
            "pub struct Todo;\nimpl RustLintPass for Todo {}\nimpl TypeScriptLintPass for Todo {}",
        );

        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn pass_in_test_code_is_not_flagged() {
        let diagnostics = run(
            "#[cfg(test)]\nmod tests {\n    pub struct Fake;\n    impl RustLintPass for Fake {}\n}",
        );

        assert_no_diagnostics(&diagnostics);
    }

    #[test]
    fn pass_is_send_sync_and_unpin() {
        assert_auto_traits::<IncompletePassDoc>();
    }

    #[test]
    fn pass_with_a_summary_and_examples_only_is_flagged_for_its_body() {
        let found = missing("Flags a thing\n\n# Examples\n\n```text\n// DON'T\n// DO\n```");

        assert_eq!(found, Some(MissingPart::Body));
    }

    #[test]
    fn pass_with_only_a_summary_is_flagged_for_its_body() {
        let diagnostics = run("/// Flags a thing\npub struct Todo;\nimpl RustLintPass for Todo {}");

        assert_diagnostic(&diagnostics[0])
            .has_rule_id("lint.incomplete-pass-doc")
            .message_contains("stop at the summary");
    }

    #[test]
    fn pass_without_docs_is_flagged_on_its_name() {
        let source = "pub struct Todo;\nimpl RustLintPass for Todo {}";

        let diagnostics = run(source);

        assert_diagnostic(&diagnostics[0])
            .has_severity(Severity::Error)
            .has_span("<test>", 11..15);
    }

    #[test]
    fn pass_without_examples_is_flagged_for_them() {
        let found = missing("Flags a thing\n\nBody");

        assert_eq!(found, Some(MissingPart::Examples));
    }

    #[test]
    fn private_pass_is_not_flagged() {
        let diagnostics = run("struct Todo;\nimpl RustLintPass for Todo {}");

        assert_no_diagnostics(&diagnostics);
    }

    #[test]
    fn struct_without_a_lint_pass_impl_is_not_flagged() {
        let diagnostics = run("pub struct Todo;\nimpl Todo {}");

        assert_no_diagnostics(&diagnostics);
    }
}
