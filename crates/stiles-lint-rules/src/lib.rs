//! The entry point to the Stiles rules for crates of Stiles rules
//!
//! Each module below is one rule. It holds the rule's pass and its
//! `RULE_ID`. The docs of the pass say what the rule reports and why.
//!
//! This crate also gives the workspace its name. Otto releases the
//! workspace as one lockstep unit, and such a unit needs a crate with the
//! name of its repository. Every crate inherits the version of the
//! workspace, so one release moves the facade and every rule together.
//!
//! A project loads the rules from a commit of this repository, so no crate
//! is published.

pub use incomplete_pass_doc;
pub use undeclared_rule_id;
pub use undocumented_option;
