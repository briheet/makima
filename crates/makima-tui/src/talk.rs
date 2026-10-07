//! Routing what you type in the talk line: `@lead`, `@all`, `@children`,
//! `@2` (a child's number) or `@claude-2` (a name, spaces as dashes),
//! followed by the message.

use makima_core::AgentId;

/// Who a message is for.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Target {
    Lead,
    All,
    Children,
    /// A child by number (1-based) or an agent by name.
    Agent(String),
}

/// An agent that can receive messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipient {
    pub id: AgentId,
    pub name: String,
}

/// A message and the agents it goes to.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Routed {
    pub recipients: Vec<AgentId>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum TalkError {
    #[error("nothing to send")]
    Empty,
    #[error("no agent called @{0}")]
    UnknownAgent(String),
    #[error("there are no agents to talk to")]
    NoAgents,
}

/// Splits a leading `@target` off `input`, falling back to `default`.
fn parse(input: &str, default: Target) -> (Target, &str) {
    let trimmed = input.trim_start();
    let Some(rest) = trimmed.strip_prefix('@') else {
        return (default, trimmed);
    };
    let (word, text) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
    let target = match word.to_lowercase().as_str() {
        "lead" | "leader" => Target::Lead,
        "all" | "everyone" => Target::All,
        "children" | "kids" | "team" => Target::Children,
        _ => Target::Agent(word.to_owned()),
    };
    (target, text.trim_start())
}

/// Resolves `input` against a work's `lead` and `children`.
///
/// # Errors
/// [`TalkError`] for an empty message, an unknown `@name`, or no agents.
pub fn route_talk(
    input: &str,
    default: Target,
    lead: Option<&Recipient>,
    children: &[Recipient],
) -> Result<Routed, TalkError> {
    let (target, text) = parse(input, default);
    if text.trim().is_empty() {
        return Err(TalkError::Empty);
    }
    let ids =
        |agents: &mut dyn Iterator<Item = &Recipient>| agents.map(|r| r.id).collect::<Vec<_>>();
    let recipients = match &target {
        Target::Lead => ids(&mut lead.into_iter()),
        Target::All => ids(&mut lead.into_iter().chain(children)),
        Target::Children => ids(&mut children.iter()),
        Target::Agent(word) => {
            let by_number = word
                .parse::<usize>()
                .ok()
                .and_then(|n| n.checked_sub(1))
                .and_then(|i| children.get(i));
            let by_name = || {
                lead.into_iter()
                    .chain(children)
                    .find(|r| r.name.replace(' ', "-").eq_ignore_ascii_case(word))
            };
            let found = by_number
                .or_else(by_name)
                .ok_or_else(|| TalkError::UnknownAgent(word.clone()))?;
            vec![found.id]
        }
    };
    if recipients.is_empty() {
        return Err(TalkError::NoAgents);
    }
    Ok(Routed {
        recipients,
        text: text.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recipient(name: &str) -> Recipient {
        Recipient {
            id: AgentId::new(),
            name: name.into(),
        }
    }

    struct Team {
        lead: Recipient,
        children: [Recipient; 2],
    }

    impl Team {
        fn new() -> Self {
            Self {
                lead: recipient("claude"),
                children: [recipient("claude 2"), recipient("codex")],
            }
        }

        fn route(&self, input: &str) -> Result<Routed, TalkError> {
            route_talk(input, Target::Lead, Some(&self.lead), &self.children)
        }

        fn to(&self, input: &str) -> Result<Vec<AgentId>, TalkError> {
            self.route(input).map(|r| r.recipients)
        }
    }

    fn route(input: &str) -> Result<Routed, TalkError> {
        Team::new().route(input)
    }

    #[test]
    fn targets() {
        let team = Team::new();
        let (l, a, b) = (team.lead.id, team.children[0].id, team.children[1].id);
        assert_eq!(team.to("fix it"), Ok(vec![l]));
        assert_eq!(team.to("@all fix it"), Ok(vec![l, a, b]));
        assert_eq!(team.to("@children go"), Ok(vec![a, b]));
        assert_eq!(team.to("@2 go"), Ok(vec![b]));
        assert_eq!(team.to("@claude-2 go"), Ok(vec![a]));
        assert_eq!(
            route("@all   summarise").map(|r| r.text),
            Ok("summarise".into())
        );
    }

    #[test]
    fn mistakes() {
        assert_eq!(route("@all"), Err(TalkError::Empty));
        assert_eq!(
            route("@nobody hi"),
            Err(TalkError::UnknownAgent("nobody".into()))
        );
        assert_eq!(
            route_talk("@children hi", Target::Lead, None, &[]),
            Err(TalkError::NoAgents)
        );
    }
}
