//! Templates: a kind of work's starting team and the questions its brief
//! asks. Built-ins cover each [`WorkKind`]; the config can replace them or
//! add more.

use crate::preset::DEFAULT_LEAD;
use crate::prompt::{DEFAULT_CHILD_PROMPT, DEFAULT_LEAD_PROMPT};
use crate::{AgentRole, PermissionMode, PresetId, WorkKind};

/// One member of a template's team.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct TeamMember {
    /// `None` runs whatever the lead runs.
    pub preset: Option<PresetId>,
    pub role: AgentRole,
}

impl TeamMember {
    #[must_use]
    pub fn new(preset: Option<PresetId>, role: AgentRole) -> Self {
        Self { preset, role }
    }
}

/// A question the brief asks, e.g. "What's broken?".
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct BriefQuestion {
    pub label: String,
    /// Shown dimmed in the empty field.
    pub hint: String,
}

impl BriefQuestion {
    #[must_use]
    pub fn new(label: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            hint: hint.into(),
        }
    }
}

/// A starting point for new work: team shape plus brief questions.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Template {
    pub id: String,
    pub kind: Option<WorkKind>,
    pub label: String,
    pub description: String,
    /// The lead's preset.
    pub lead: PresetId,
    pub children: Vec<TeamMember>,
    /// Agents should read, not change files.
    pub read_only: bool,
    /// The team's permission mode; `None` uses the config's.
    pub permission: Option<PermissionMode>,
    pub questions: Vec<BriefQuestion>,
    /// What the lead is told at start (see `makima_core::prompt`).
    pub lead_prompt: String,
    /// What each child is told at start.
    pub child_prompt: String,
}

impl Template {
    #[must_use]
    pub fn new(id: impl Into<String>, label: impl Into<String>, lead: impl Into<PresetId>) -> Self {
        Self {
            id: id.into(),
            kind: None,
            label: label.into(),
            description: String::new(),
            lead: lead.into(),
            children: Vec::new(),
            read_only: false,
            permission: None,
            questions: Vec::new(),
            lead_prompt: DEFAULT_LEAD_PROMPT.to_owned(),
            child_prompt: DEFAULT_CHILD_PROMPT.to_owned(),
        }
    }

    /// "lead + implementer + tester".
    #[must_use]
    pub fn team_summary(&self) -> String {
        std::iter::once("lead".to_owned())
            .chain(self.children.iter().map(|m| m.role.to_string()))
            .collect::<Vec<_>>()
            .join(" + ")
    }
}

fn member(role: AgentRole) -> TeamMember {
    TeamMember::new(None, role)
}

fn question(label: &str, hint: &str) -> BriefQuestion {
    BriefQuestion::new(label, hint)
}

fn kind_template(
    kind: WorkKind,
    label: &str,
    children: Vec<TeamMember>,
    read_only: bool,
    questions: Vec<BriefQuestion>,
) -> Template {
    Template {
        id: kind.id().into(),
        kind: Some(kind),
        label: label.into(),
        description: kind.description().into(),
        lead: DEFAULT_LEAD.into(),
        children,
        read_only,
        // Read-only work plans; it never needs to edit.
        permission: read_only.then_some(PermissionMode::Plan),
        questions,
        lead_prompt: lead_prompt(kind).to_owned(),
        child_prompt: DEFAULT_CHILD_PROMPT.to_owned(),
    }
}

/// What the lead of each kind of work is told to do first.
fn lead_prompt(kind: WorkKind) -> &'static str {
    match kind {
        WorkKind::Feat => {
            "You are the lead on \"{name}\": adding a feature in {folder}. Read the brief at \
             {brief}. Build what the brief asks and nothing it rules out; done means its \
             \"done when\" checks pass, with tests. {protocol}"
        }
        WorkKind::Fix => {
            "You are the lead on \"{name}\": fixing a bug in {folder}. Read the brief at {brief}. \
             Reproduce the bug first, find the root cause (not just the symptom), make the \
             smallest fix that removes it, and have the tester prove it with a test that failed \
             before. {protocol}"
        }
        WorkKind::Refactor => {
            "You are the lead on \"{name}\": restructuring code in {folder} without changing \
             behaviour. Read the brief at {brief}. Work in small steps that each keep the tests \
             passing, never touch what the brief says must not change, and have the reviewer \
             check each step. {protocol}"
        }
        WorkKind::Research => {
            "You are the lead on \"{name}\": answering a question about {folder}. Read the brief \
             at {brief}. Give each researcher a different angle so no two repeat each other, \
             change no files, cite where each finding came from, and finish with what the brief \
             asks to be delivered. {protocol}"
        }
        WorkKind::Review => {
            "You are the lead on \"{name}\": reviewing code in {folder}. Read the brief at \
             {brief}. Split the review by area so reviewers don't overlap, change no files, and \
             report the findings most serious first, each with where it is and why it matters. \
             {protocol}"
        }
        WorkKind::Docs => {
            "You are the lead on \"{name}\": writing documentation in {folder}. Read the brief \
             at {brief}. Write for the readers the brief names, put it where the brief says, and \
             check every claim against the code. {protocol}"
        }
    }
}

/// One template per [`WorkKind`], in display order.
#[must_use]
pub fn builtin_templates() -> Vec<Template> {
    vec![
        kind_template(
            WorkKind::Feat,
            "Feature",
            vec![member(AgentRole::Implementer), member(AgentRole::Tester)],
            false,
            vec![
                question("What should it do?", "the outcome, in a sentence or two"),
                question(
                    "Done when",
                    "checks anyone can run: a test, a command, a behaviour",
                ),
                question("Not in scope", "what to leave alone"),
                question("Constraints", "files, APIs or approaches to use or avoid"),
            ],
        ),
        kind_template(
            WorkKind::Fix,
            "Fix",
            vec![member(AgentRole::Tester)],
            false,
            vec![
                question("What's broken?", "the symptom"),
                question("How to reproduce", "steps, commands, inputs"),
                question("Expected instead", "what should happen"),
                question("Where to look", "files, logs, recent changes"),
            ],
        ),
        kind_template(
            WorkKind::Refactor,
            "Refactor",
            vec![member(AgentRole::Reviewer)],
            false,
            vec![
                question("What to restructure", "modules, files, patterns"),
                question("Why", "the problem with how it is now"),
                question("Must not change", "behaviour and public APIs to keep"),
                question("Done when", "how we'll know it's better"),
            ],
        ),
        kind_template(
            WorkKind::Research,
            "Research",
            vec![member(AgentRole::Researcher)],
            true,
            vec![
                question("The question", "what do you want to find out?"),
                question("Where to look", "code, docs, links"),
                question("What to deliver", "a summary, a recommendation, a doc…"),
            ],
        ),
        kind_template(
            WorkKind::Review,
            "Review",
            vec![member(AgentRole::Reviewer), member(AgentRole::Reviewer)],
            true,
            vec![
                question("What to review", "a branch, PR, or paths"),
                question("Focus on", "correctness, security, performance…"),
                question("What to deliver", "findings ranked by severity, a verdict…"),
            ],
        ),
        kind_template(
            WorkKind::Docs,
            "Docs",
            Vec::new(),
            false,
            vec![
                question("What's missing?", "the doc to write or fix"),
                question("Who reads it?", "the audience"),
                question("Where it goes", "the file or section"),
            ],
        ),
    ]
}

/// `base` with `overrides` applied by id, like presets.
#[must_use]
pub fn merge_templates(mut base: Vec<Template>, overrides: Vec<Template>) -> Vec<Template> {
    for template in overrides {
        match base.iter_mut().find(|t| t.id == template.id) {
            Some(existing) => *existing = template,
            None => base.push(template),
        }
    }
    base
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_a_template() {
        let templates = builtin_templates();
        for kind in WorkKind::ALL {
            assert!(templates.iter().any(|t| t.kind == Some(kind)), "{kind}");
        }
        let fix = &templates[1];
        assert_eq!(fix.team_summary(), "lead + tester");
        assert!(
            templates
                .iter()
                .find(|t| t.id == "research")
                .is_some_and(|t| t.read_only)
        );
    }
}
