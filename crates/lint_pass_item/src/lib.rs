//! Reads the items of a lint crate that the rules here check
//!
//! A lint crate defines a pass as a struct and implements a lint pass trait
//! for it, such as `RustLintPass`. The docs of the struct describe the rule,
//! the pass reads its options, and an implementation of `DeclaresRules`
//! names the rule ids it reports. Each rule here reads some of these items
//! from one file, so this crate finds them in a syntax tree.

mod test_code;

use std::collections::BTreeMap;

use stiles_types::DecoratedNode;

pub use self::test_code::is_test_code;

/// The suffix that every lint pass trait of stiles carries in its name
const LINT_PASS_SUFFIX: &str = "LintPass";

/// Returns every node of one kind under a node, in source order
///
/// # Examples
///
/// ```
/// use lint_pass_item::descendants;
/// use stiles_testing::parse;
/// use stiles_types::Language;
///
/// let tree = parse("struct A;\nmod b { struct C; }", Language::Rust);
///
/// assert_eq!(descendants(tree.root_node(), "struct_item").len(), 2);
/// ```
pub fn descendants<'a>(node: DecoratedNode<'a>, kind: &str) -> Vec<DecoratedNode<'a>> {
    let mut found = Vec::new();
    let mut pending = vec![node];
    while let Some(current) = pending.pop() {
        if current.kind() == kind {
            found.push(current);
        }
        pending.extend(current.named_children().into_iter().rev());
    }
    found
}

/// Returns the last path segment of the trait an `impl` block implements
///
/// The name loses its generic arguments, so `stiles_rust::RustLintPass` and
/// `RustLintPass` both read as `RustLintPass`. An inherent `impl` has no
/// trait.
///
/// # Examples
///
/// ```
/// use lint_pass_item::implemented_trait;
/// use stiles_testing::parse;
/// use stiles_types::Language;
///
/// let tree = parse("impl stiles_rust::RustLintPass for Todo {}", Language::Rust);
/// let block = tree.root_node().named_child(0).unwrap();
///
/// assert_eq!(implemented_trait(&block), Some("RustLintPass"));
/// ```
pub fn implemented_trait<'a>(block: &DecoratedNode<'a>) -> Option<&'a str> {
    block
        .child_by_field_name("trait")
        .map(|path| last_segment(path.text()))
}

/// Returns the last path segment of the type an `impl` block is for
///
/// # Examples
///
/// ```
/// use lint_pass_item::implementing_type;
/// use stiles_testing::parse;
/// use stiles_types::Language;
///
/// let tree = parse("impl RustLintPass for Todo<'_> {}", Language::Rust);
/// let block = tree.root_node().named_child(0).unwrap();
///
/// assert_eq!(implementing_type(&block), Some("Todo"));
/// ```
pub fn implementing_type<'a>(block: &DecoratedNode<'a>) -> Option<&'a str> {
    block
        .child_by_field_name("type")
        .map(|path| last_segment(path.text()))
}

/// Returns whether a trait name names a lint pass trait of stiles
///
/// Every such trait ends in `LintPass`: `LintPass` itself, `RustLintPass`,
/// and `TypeScriptLintPass`.
///
/// # Examples
///
/// ```
/// use lint_pass_item::is_lint_pass_trait;
///
/// assert!(is_lint_pass_trait("TypeScriptLintPass"));
/// ```
pub fn is_lint_pass_trait(name: &str) -> bool {
    name.ends_with(LINT_PASS_SUFFIX)
}

/// Returns the `impl` blocks of lint pass traits outside test code
///
/// A test defines passes of its own to exercise stiles, and those passes
/// describe no rule.
///
/// # Examples
///
/// ```
/// use lint_pass_item::lint_pass_impls;
/// use stiles_testing::parse;
/// use stiles_types::Language;
///
/// let tree = parse("impl RustLintPass for Todo {}\nimpl Todo {}", Language::Rust);
///
/// assert_eq!(lint_pass_impls(tree.root_node()).len(), 1);
/// ```
pub fn lint_pass_impls(root: DecoratedNode<'_>) -> Vec<DecoratedNode<'_>> {
    descendants(root, "impl_item")
        .into_iter()
        .filter(|block| implemented_trait(block).is_some_and(is_lint_pass_trait))
        .filter(|block| !is_test_code(block))
        .collect()
}

/// Returns the struct with a given name under a node
///
/// # Examples
///
/// ```
/// use lint_pass_item::struct_named;
/// use stiles_testing::parse;
/// use stiles_types::Language;
///
/// let tree = parse("struct Todo;", Language::Rust);
///
/// assert!(struct_named(tree.root_node(), "Todo").is_some());
/// ```
pub fn struct_named<'a>(root: DecoratedNode<'a>, name: &str) -> Option<DecoratedNode<'a>> {
    descendants(root, "struct_item").into_iter().find(|item| {
        item.child_by_field_name("name")
            .is_some_and(|identifier| identifier.text() == name)
    })
}

/// Returns the lines of an item's outer doc comment, without their markers
///
/// The comment is the run of `///` lines and `/** */` blocks above the
/// item. Attributes and plain comments may sit between them and the item,
/// as rustdoc allows. A `////` line is a plain comment.
///
/// # Examples
///
/// ```
/// use lint_pass_item::doc_lines;
/// use stiles_testing::parse;
/// use stiles_types::Language;
///
/// let tree = parse("/// Flags a thing\n#[derive(Default)]\nstruct Todo;", Language::Rust);
/// let item = tree.root_node().named_child(2).unwrap();
///
/// assert_eq!(doc_lines(&item), vec!["Flags a thing"]);
/// ```
pub fn doc_lines(item: &DecoratedNode<'_>) -> Vec<String> {
    let Some(parent) = item.parent() else {
        return Vec::new();
    };
    let siblings = parent.named_children();
    let Some(index) = siblings
        .iter()
        .position(|sibling| sibling.id() == item.id())
    else {
        return Vec::new();
    };

    let mut blocks = Vec::new();
    for sibling in siblings[..index].iter().rev() {
        match sibling.kind() {
            "attribute_item" => {}
            "line_comment" | "block_comment" => blocks.push(doc_text(sibling.text())),
            _ => break,
        }
    }
    blocks.into_iter().rev().flatten().collect()
}

/// Returns the doc lines that one comment holds
///
/// A comment that is not a doc comment holds none.
fn doc_text(comment: &str) -> Vec<String> {
    if let Some(line) = comment.strip_prefix("///") {
        if line.starts_with('/') {
            return Vec::new();
        }
        let line = line.trim_end_matches(['\r', '\n']);
        return vec![line.strip_prefix(' ').unwrap_or(line).to_string()];
    }

    let Some(body) = comment
        .strip_prefix("/**")
        .and_then(|rest| rest.strip_suffix("*/"))
    else {
        return Vec::new();
    };
    if body.starts_with('*') {
        return Vec::new();
    }
    body.lines()
        .map(|line| {
            let line = line.trim_start();
            let line = line.strip_prefix('*').unwrap_or(line);
            line.strip_prefix(' ')
                .unwrap_or(line)
                .trim_end()
                .to_string()
        })
        .skip_while(|line| line.is_empty())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .skip_while(|line| line.is_empty())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

/// Returns the text inside a plain string literal
///
/// A raw string or a string with an escape returns nothing, because its
/// source text is not its value.
///
/// # Examples
///
/// ```
/// use lint_pass_item::string_value;
/// use stiles_testing::parse;
/// use stiles_types::Language;
///
/// let tree = parse("const A: &str = \"boundary-attributes\";", Language::Rust);
/// let item = tree.root_node().named_child(0).unwrap();
/// let literal = item.child_by_field_name("value").unwrap();
///
/// assert_eq!(string_value(&literal), Some("boundary-attributes"));
/// ```
pub fn string_value<'a>(literal: &DecoratedNode<'a>) -> Option<&'a str> {
    if literal.kind() != "string_literal" {
        return None;
    }
    let inner = literal.text().strip_prefix('"')?.strip_suffix('"')?;
    match inner.contains('\\') {
        true => None,
        false => Some(inner),
    }
}

/// Returns the string that each constant in a file holds, by name
///
/// A constant holds a string when its value is a string literal, or a call
/// whose only argument is one, such as `RuleId::new("lint.todo-comment")`.
/// A constant that holds anything else is left out.
///
/// # Examples
///
/// ```
/// use lint_pass_item::string_constants;
/// use stiles_testing::parse;
/// use stiles_types::Language;
///
/// let tree = parse("const RULE_ID: RuleId = RuleId::new(\"lint.todo\");", Language::Rust);
/// let constants = string_constants(tree.root_node());
///
/// assert_eq!(constants.get("RULE_ID"), Some(&"lint.todo"));
/// ```
pub fn string_constants(root: DecoratedNode<'_>) -> BTreeMap<&str, &str> {
    descendants(root, "const_item")
        .into_iter()
        .filter_map(|item| {
            let name = item.child_by_field_name("name")?.text();
            let value = item.child_by_field_name("value")?;
            Some((name, wrapped_string(&value)?))
        })
        .collect()
}

/// Returns the string a value holds, alone or as the only call argument
///
/// # Examples
///
/// ```
/// use lint_pass_item::wrapped_string;
/// use stiles_testing::parse;
/// use stiles_types::Language;
///
/// let tree = parse("fn f() { RuleId::new(\"lint.todo\"); }", Language::Rust);
/// let body = tree.root_node().named_child(0).unwrap().child_by_field_name("body").unwrap();
/// let call = body.named_child(0).unwrap().named_child(0).unwrap();
///
/// assert_eq!(wrapped_string(&call), Some("lint.todo"));
/// ```
pub fn wrapped_string<'a>(value: &DecoratedNode<'a>) -> Option<&'a str> {
    if value.kind() != "call_expression" {
        return string_value(value);
    }
    let arguments = value.child_by_field_name("arguments")?.named_children();
    match arguments.as_slice() {
        [only] => string_value(only),
        _ => None,
    }
}

/// Returns the last segment of a path, without its generic arguments
fn last_segment(path: &str) -> &str {
    let path = path.split('<').next().unwrap_or(path);
    path.rsplit("::").next().unwrap_or(path).trim()
}

#[cfg(test)]
mod tests {
    use stiles_testing::parse;
    use stiles_types::Language;

    use super::*;

    fn first_struct_docs(source: &str) -> Vec<String> {
        let tree = parse(source, Language::Rust);
        let item = descendants(tree.root_node(), "struct_item")
            .into_iter()
            .next()
            .expect("the source should hold a struct");
        doc_lines(&item)
    }

    #[test]
    fn doc_lines_with_a_block_doc_comment_reads_its_lines() {
        let docs = first_struct_docs("/**\n * Flags a thing\n *\n * More\n */\nstruct A;");

        assert_eq!(docs, vec!["Flags a thing", "", "More"]);
    }

    #[test]
    fn doc_lines_with_a_plain_comment_between_skips_it() {
        let docs = first_struct_docs("/// Flags a thing\n// note\nstruct A;");

        assert_eq!(docs, vec!["Flags a thing"]);
    }

    #[test]
    fn doc_lines_with_an_item_between_stops_there() {
        let docs = first_struct_docs("/// Flags a thing\nconst B: u8 = 1;\nstruct A;");

        assert!(docs.is_empty());
    }

    #[test]
    fn doc_lines_with_four_slashes_reads_nothing() {
        let docs = first_struct_docs("//// Flags a thing\nstruct A;");

        assert!(docs.is_empty());
    }

    #[test]
    fn last_segment_with_generics_drops_them() {
        let segment = last_segment("stiles::Pass<'a, T>");

        assert_eq!(segment, "Pass");
    }

    #[test]
    fn lint_pass_impls_in_a_test_module_skips_them() {
        let tree = parse(
            "#[cfg(test)]\nmod tests {\n    impl RustLintPass for Fake {}\n}",
            Language::Rust,
        );

        let blocks = lint_pass_impls(tree.root_node());

        assert!(blocks.is_empty());
    }

    #[test]
    fn string_constants_with_a_non_string_value_skips_it() {
        let tree = parse("const A: u8 = 1;", Language::Rust);

        let constants = string_constants(tree.root_node());

        assert!(constants.is_empty());
    }

    #[test]
    fn string_value_with_an_escape_returns_none() {
        let tree = parse("const A: &str = \"a\\nb\";", Language::Rust);
        let item = tree
            .root_node()
            .named_child(0)
            .expect("the source should hold a constant");
        let literal = item
            .child_by_field_name("value")
            .expect("the constant should have a value");

        let value = string_value(&literal);

        assert!(value.is_none());
    }

    #[test]
    fn wrapped_string_with_a_bare_literal_returns_it() {
        let tree = parse("const A: &str = \"a\";", Language::Rust);
        let item = tree
            .root_node()
            .named_child(0)
            .expect("the source should hold a constant");
        let value = item
            .child_by_field_name("value")
            .expect("the constant should have a value");

        let string = wrapped_string(&value);

        assert_eq!(string, Some("a"));
    }

    #[test]
    fn wrapped_string_with_two_arguments_returns_none() {
        let tree = parse("const A: Id = Id::new(\"a\", \"b\");", Language::Rust);
        let item = tree
            .root_node()
            .named_child(0)
            .expect("the source should hold a constant");
        let value = item
            .child_by_field_name("value")
            .expect("the constant should have a value");

        let string = wrapped_string(&value);

        assert!(string.is_none());
    }
}
