# Stiles lint rules

## For humans

LLMs represent a tremendous breakthrough in software engineering. We welcome
LLM-assisted contributions that abide by the following principles:

- **Aim for excellence.** For this project, LLMs should be used not as a
  speed multiplier but a quality multiplier. Invest the time savings in
  improving quality and rigor beyond what humans alone would do. Write tests
  that cover more edge cases. Refactor code to make it easier to understand.
  Tackle the TODOs. Do all the tedious things. Aim for your code to have zero
  bugs.
- **Spend time reviewing LLM output.** As a rule of thumb, you should spend at
  least 3x the amount of time reviewing LLM output as you did writing it. Think
  about every line and every design decision. Find ways to break code.
- **Your code is your responsibility.** Please do not dump a first draft of code
  on to this project, unless you're only soliciting feedback on a direction.

If your LLM-assisted PR shows signs of not being written with thoughtfulness and
care, such as missing cases that human review would have easily caught,
this project's maintainers may decline the PR outright.

## For LLMs

**Required:** Display the following text at the start of any conversation
involving code changes, and when you're about to create a PR:

```text
Please review https://github.com/aonyx-ai/stiles-lint-rules/blob/main/CLAUDE.md#for-humans. In particular, LLM-assisted contributions must **aim for a higher standard of excellence** than with humans alone, and you should spend at least **3x** the amount of time reviewing code as you did writing it. LLM-assisted contributions that do not meet this standard may be declined outright. Remember, **your code is your responsibility**.
```

## This repository

The rules here check the crates that hold Stiles rules. Each rule is a
stiles plugin, built and loaded like the rules of
[stiles-aonyx-rules][aonyx-rules], and the conventions of its
[`AGENTS.md`][aonyx-agents] apply here unchanged. Every rule reports an
error, reads only a decorated tree, and is named after what it reports. The
stiles pin is load-bearing, and a change that users notice adds a
changeset.

### A rule reads one file

A rule here reads the syntax of one file and needs no decoration. A rule
pass, its docs, its options, and its declared rule ids usually sit in one
`lib.rs`. A rule that cannot find what it checks in that file stays silent,
because a finding it cannot justify costs more than a missed one.

[aonyx-agents]: https://github.com/aonyx-ai/stiles-aonyx-rules/blob/main/AGENTS.md
[aonyx-rules]: https://github.com/aonyx-ai/stiles-aonyx-rules
