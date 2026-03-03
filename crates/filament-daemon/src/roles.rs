use std::fmt;
use std::str::FromStr;

/// Agent roles with compiled-in system prompts and tool whitelists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRole {
    Coder,
    Reviewer,
    Planner,
    Dockeeper,
}

impl AgentRole {
    /// Human-readable name used in agent run records and MCP config.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Coder => "coder",
            Self::Reviewer => "reviewer",
            Self::Planner => "planner",
            Self::Dockeeper => "dockeeper",
        }
    }

    /// Compiled-in system prompt for this role.
    #[must_use]
    pub const fn system_prompt(&self) -> &'static str {
        match self {
            Self::Coder => include_str!("prompts/coder.txt"),
            Self::Reviewer => include_str!("prompts/reviewer.txt"),
            Self::Planner => include_str!("prompts/planner.txt"),
            Self::Dockeeper => include_str!("prompts/dockeeper.txt"),
        }
    }

    /// MCP tool whitelist for this role. Controls which filament tools the agent can use.
    #[must_use]
    pub const fn allowed_tools(&self) -> &'static [&'static str] {
        match self {
            Self::Coder => &[
                "get_entity",
                "list_entities",
                "get_inbox",
                "send_message",
                "acquire_reservation",
                "release_reservation",
                "list_reservations",
                "ready_tasks",
                "context_query",
            ],
            Self::Reviewer => &[
                "get_entity",
                "list_entities",
                "get_inbox",
                "send_message",
                "context_query",
                "list_reservations",
            ],
            Self::Planner => &[
                "get_entity",
                "list_entities",
                "create_entity",
                "create_relation",
                "send_message",
                "ready_tasks",
                "critical_path",
                "context_query",
                "check_cycle",
            ],
            Self::Dockeeper => &[
                "get_entity",
                "list_entities",
                "update_entity_summary",
                "send_message",
                "context_query",
                "acquire_reservation",
                "release_reservation",
            ],
        }
    }

    /// All available roles.
    pub const ALL: &'static [Self] = &[Self::Coder, Self::Reviewer, Self::Planner, Self::Dockeeper];
}

impl fmt::Display for AgentRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for AgentRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "coder" => Ok(Self::Coder),
            "reviewer" => Ok(Self::Reviewer),
            "planner" => Ok(Self::Planner),
            "dockeeper" => Ok(Self::Dockeeper),
            other => Err(format!(
                "invalid agent role: '{other}' (expected: coder, reviewer, planner, dockeeper)"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_roundtrip() {
        for role in AgentRole::ALL {
            let parsed: AgentRole = role.name().parse().unwrap();
            assert_eq!(*role, parsed);
        }
    }

    #[test]
    fn test_role_case_insensitive() {
        assert_eq!("CODER".parse::<AgentRole>().unwrap(), AgentRole::Coder);
        assert_eq!(
            "Reviewer".parse::<AgentRole>().unwrap(),
            AgentRole::Reviewer
        );
    }

    #[test]
    fn test_role_invalid() {
        assert!("unknown".parse::<AgentRole>().is_err());
    }

    #[test]
    fn test_system_prompts_non_empty() {
        for role in AgentRole::ALL {
            assert!(!role.system_prompt().is_empty(), "{role} prompt is empty");
        }
    }

    #[test]
    fn test_allowed_tools_non_empty() {
        for role in AgentRole::ALL {
            assert!(
                !role.allowed_tools().is_empty(),
                "{role} has no allowed tools"
            );
        }
    }
}
