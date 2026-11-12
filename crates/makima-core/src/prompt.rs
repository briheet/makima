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
//! | `{protocol}` | how the lead runs the team (see below) |
//! | `{report}` | how a child reports back (see below) |
//! | `{me}` | the agent's session name, e.g. `auth-1a2b-child-2` |
//! | `{lead}` | the lead's session name |
//! | `{peers}` | every session on the team, by number and role |
//!
//! The agents are separate sessions that can't see each other's
//! conversations. When every one of them runs a program that can message
//! by session name (Claude Code), they talk directly: the lead starts the
//! children, and children report to the lead and ask each other for what
//! they need. Otherwise they coordinate through `plan.md` alone and the
//! user says "go". Either way `plan.md` is the shared record (after
//! Anthropic's multi-agent research: each child needs an objective, the
//! files it owns, and how to report).

use crate::{AgentRole, WorkspaceId};

/// The lead's plan steps, shared by both protocols.
const PLAN_STEPS: &str = "1) Plan first: split the work into tasks, each with an outcome, the \
files it may touch, and how to check it's done. Give no two tasks the same files. Small work \
needs fewer people: leave a child idle rather than split what one agent can do. \
2) Write {plan} with a section per child, headed \"## child N\", holding its task, its files, \
and its done-when.";

/// How the lead runs a team that can't message: through `plan.md`, with
/// the user starting the children.
pub const LEAD_PROTOCOL: &str = "Your team: {team}. They are separate sessions: you can't \
message them and they can't see this conversation, so coordinate through {plan}. {steps} \
3) Then tell the user the plan is ready, so they can tell everyone to go. \
4) Do your own share, read the status lines the children add under their sections, and finish \
when the brief's done-when holds.";

/// How the lead runs a team that can message: it starts the children and
/// talks to them directly.
pub const LEAD_MESSAGING_PROTOCOL: &str = "Your team: {team}. Each is its own Claude Code \
session on this machine, and you are {me}: {peers}. They can't see this conversation. Message \
them with SendMessage, addressed by session name (ListAgents lists them); their messages reach \
you the same way, between your tool calls or as a new turn. {steps} \
3) Then message each child to start, pointing it at its section; don't wait for the user. \
4) Do your own share, answer the children's questions, keep {plan} up to date with who has done \
what, and finish when the brief's done-when holds. Then tell the user.";

/// How a child reports when the team can't message.
pub const CHILD_REPORT: &str = "Add one status line under your section when you start, when \
you're blocked, and when you're done.";

/// How a child reports when the team can message.
pub const CHILD_MESSAGING_REPORT: &str = "You are {me}; the lead is {lead}, and the team is \
{peers}. Use SendMessage, addressed by session name: tell the lead when you start, when you're \
blocked, and when you're done, and message a teammate directly when your work needs theirs (a \
review, a file they own). Their messages reach you on their own: act on them. Also add one \
status line under your section in {plan} at each of those points.";

/// The lead's prompt when a template doesn't say.
pub const DEFAULT_LEAD_PROMPT: &str = "You are the lead on \"{name}\", a {kind} in {folder}. \
Read the brief at {brief}. {protocol}";

/// A child's prompt when a template doesn't say.
pub const DEFAULT_CHILD_PROMPT: &str = "You are child {number}, the {role}, on \"{name}\", a \
{kind} in {folder}. {duty} You can't see the lead's conversation; you work from files. The \
brief is at {brief}, and your task will be under \"## child {number}\" in {plan}. Until you're \
told to go, reply only with \"ready\". When you work, touch only the files your task names. \
{report}";

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
    /// This agent's session name.
    pub me: &'a str,
    /// The lead's session name.
    pub lead: &'a str,
    /// The team's sessions, e.g. `lead auth-1a2b-lead; child 1 (tester)
    /// auth-1a2b-child-1`.
    pub peers: &'a str,
    /// Every agent on the team can message by session name.
    pub messaging: bool,
}

/// `template` with its placeholders filled; unknown ones are left as they
/// are, and extra spaces from empty values are tidied.
#[must_use]
pub fn render(template: &str, vars: &PromptVars<'_>) -> String {
    // The protocols go in first, so their own placeholders are filled too.
    let (protocol, report) = if vars.messaging {
        (LEAD_MESSAGING_PROTOCOL, CHILD_MESSAGING_REPORT)
    } else {
        (LEAD_PROTOCOL, CHILD_REPORT)
    };
    let filled = [
        ("{protocol}", protocol),
        ("{report}", report),
        ("{steps}", PLAN_STEPS),
        ("{me}", vars.me),
        ("{lead}", vars.lead),
        ("{peers}", vars.peers),
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

/// The session name of the agent at `position` (0 is the lead) in a work:
/// `auth-refactor-1a2b-lead`, `auth-refactor-1a2b-child-2`. Readable, made
/// of letters, digits and dashes so it needs no quoting, and unique across
/// works by the end of the work's id.
#[must_use]
pub fn session_name(work: &str, work_id: WorkspaceId, position: usize) -> String {
    let mut slug = String::new();
    for c in work.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
        if slug.len() >= 24 {
            break;
        }
    }
    let slug = slug.trim_end_matches('-');
    let id = work_id.to_string();
    let tag = &id[id.len().saturating_sub(4)..];
    let base = if slug.is_empty() {
        format!("work-{tag}")
    } else {
        format!("{slug}-{tag}")
    };
    if position == 0 {
        format!("{base}-lead")
    } else {
        format!("{base}-child-{position}")
    }
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
            ..PromptVars::default()
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
        assert!(
            child.contains("status line under your section when you start"),
            "{child}"
        );
    }

    #[test]
    fn teams_that_can_message_talk_directly() {
        let vars = PromptVars {
            name: "fix login",
            plan: "/w/plan.md",
            number: "1",
            role: "tester",
            me: "fix-login-1a2b-child-1",
            lead: "fix-login-1a2b-lead",
            peers: "lead fix-login-1a2b-lead; child 1 (tester) fix-login-1a2b-child-1",
            messaging: true,
            ..PromptVars::default()
        };
        let lead = render(
            DEFAULT_LEAD_PROMPT,
            &PromptVars {
                me: vars.lead,
                ..vars
            },
        );
        assert!(lead.contains("you are fix-login-1a2b-lead"), "{lead}");
        assert!(lead.contains("message each child to start"), "{lead}");
        assert!(!lead.contains("tell everyone to go"), "{lead}");
        assert!(!lead.contains('{'), "every placeholder filled: {lead}");
        let child = render(DEFAULT_CHILD_PROMPT, &vars);
        assert!(
            child.contains("You are fix-login-1a2b-child-1; the lead is fix-login-1a2b-lead"),
            "{child}"
        );
        assert!(!child.contains('{'), "every placeholder filled: {child}");
    }

    #[test]
    fn session_names_are_plain_words_unique_per_work() {
        let id: WorkspaceId = "0199a1b2-0000-7000-8000-00000000c3d4"
            .parse()
            .unwrap_or_default();
        assert_eq!(
            session_name("Fix login (Safari)!", id, 0),
            "fix-login-safari-c3d4-lead"
        );
        assert_eq!(session_name("Fix login", id, 2), "fix-login-c3d4-child-2");
        assert_eq!(session_name("✓✓", id, 1), "work-c3d4-child-1");
    }
}
