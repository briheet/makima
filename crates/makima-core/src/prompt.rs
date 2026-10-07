//! Opening prompts: what each agent is told when its work starts. Templates
//! carry one for the lead and one for the children; placeholders are filled
//! in at start.
//!
//! | Placeholder | Becomes |
//! |---|---|
//! | `{name}` | the work's name |
//! | `{kind}` | its kind (`fix`, `feat`, …), or "work" |
//! | `{folder}` | where the agents work |
//! | `{brief}` | the path of `brief.md` |
//! | `{team}` | the team, e.g. `lead (claude) + tester (claude)` |
//! | `{plan}` | the path of `plan.md`, where the team coordinates |
//! | `{number}` | the child's number, as its pane shows it |
//! | `{role}` | the child's role, e.g. `tester` |
//! | `{duty}` | what that role does, e.g. "Write and run tests …" |
//!
//! The agents are separate sessions: the lead can't message its children
//! and they can't see its conversation, so they coordinate through
//! `plan.md`. The prompts say so (after Anthropic's multi-agent research:
//! each child needs an objective, the files it owns, and how to report).

use crate::AgentRole;

/// How the lead runs the team, shared by every lead prompt.
pub const LEAD_PROTOCOL: &str = "Your team: {team}. They are separate sessions: you can't \
message them and they can't see this conversation, so coordinate through {plan}. \
1) Plan first: split the work into tasks, each with an outcome, the files it may touch, and how \
to check it's done. Give no two tasks the same files. Small work needs fewer people: leave a \
child idle rather than split what one agent can do. \
2) Write {plan} with a section per child, headed \"## child N\", holding its task, its files, \
and its done-when. \
3) Then tell the user the plan is ready, so they can tell everyone to go. \
4) Do your own share, read the status lines the children add under their sections, and finish \
when the brief's done-when holds.";

/// The lead's prompt when a template doesn't say.
pub const DEFAULT_LEAD_PROMPT: &str = "You are the lead on \"{name}\", a {kind} in {folder}. \
Read the brief at {brief}. {protocol}";

/// A child's prompt when a template doesn't say.
pub const DEFAULT_CHILD_PROMPT: &str = "You are child {number}, the {role}, on \"{name}\", a \
{kind} in {folder}. {duty} You can't see the lead's conversation; you work from files. The \
brief is at {brief}, and your task will be under \"## child {number}\" in {plan}. Until you're \
told to go, reply only with \"ready\". When you work, touch only the files your task names, and \
add one status line under your section when you start, when you're blocked, and when you're \
done.";

/// What fills a prompt's placeholders.
#[derive(Debug, Clone, Copy, Default)]
pub struct PromptVars<'a> {
    pub name: &'a str,
    pub kind: &'a str,
    pub folder: &'a str,
    pub brief: &'a str,
    pub team: &'a str,
    pub plan: &'a str,
    pub number: &'a str,
    pub role: &'a str,
    pub duty: &'a str,
}

/// `template` with its placeholders filled; unknown ones are left as they
/// are, and extra spaces from empty values are tidied.
#[must_use]
pub fn render(template: &str, vars: &PromptVars<'_>) -> String {
    // The protocol goes in first, so its own placeholders are filled too.
    let filled = [
        ("{protocol}", LEAD_PROTOCOL),
        ("{name}", vars.name),
        ("{kind}", vars.kind),
        ("{folder}", vars.folder),
        ("{brief}", vars.brief),
        ("{team}", vars.team),
        ("{plan}", vars.plan),
        ("{number}", vars.number),
        ("{role}", vars.role),
        ("{duty}", vars.duty),
    ]
    .iter()
    .fold(template.to_owned(), |text, (key, value)| {
        text.replace(key, value)
    });
    filled.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// What a role does, for `{duty}`.
#[must_use]
pub fn duty(role: &AgentRole) -> &'static str {
    match role {
        AgentRole::Planner => "Plan the work and keep track of it.",
        AgentRole::Implementer => "Write the code the lead gives you, with tests.",
        AgentRole::Reviewer => {
            "Review changes for bugs, risks and unclear code; report what you find, don't rewrite it."
        }
        AgentRole::Tester => "Write and run tests that show the problem, then that it's fixed.",
        AgentRole::Researcher => "Investigate and report what you find, with sources.",
        // A shell, a custom role, or one added later: no set duty.
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_are_filled_and_spacing_tidied() {
        let vars = PromptVars {
            name: "fix login",
            kind: "fix",
            folder: "~/code/app",
            brief: "/w/brief.md",
            team: "lead (claude) + tester (claude)",
            plan: "/w/plan.md",
            number: "1",
            role: "tester",
            duty: duty(&AgentRole::Tester),
        };
        let child = render(DEFAULT_CHILD_PROMPT, &vars);
        assert!(
            child.starts_with("You are child 1, the tester, on \"fix login\""),
            "{child}"
        );
        assert!(
            child.contains("under \"## child 1\" in /w/plan.md"),
            "{child}"
        );
        assert!(child.contains("reply only with \"ready\""), "{child}");
        let lead = render(DEFAULT_LEAD_PROMPT, &vars);
        assert!(lead.contains("coordinate through /w/plan.md"), "{lead}");
        assert!(
            lead.contains("Your team: lead (claude) + tester (claude)."),
            "{lead}"
        );
        assert!(!lead.contains('{'), "every placeholder filled: {lead}");
        let shell = PromptVars {
            role: "shell",
            duty: duty(&AgentRole::Shell),
            ..vars
        };
        let custom = render("{role} {duty} done", &shell);
        assert_eq!(custom, "shell done", "an empty duty leaves no double space");
    }
}
