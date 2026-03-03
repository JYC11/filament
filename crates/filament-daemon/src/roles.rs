pub use filament_core::models::AgentRole;

/// Compiled-in system prompt for this role.
#[must_use]
pub const fn system_prompt(role: AgentRole) -> &'static str {
    match role {
        AgentRole::Coder => include_str!("prompts/coder.txt"),
        AgentRole::Reviewer => include_str!("prompts/reviewer.txt"),
        AgentRole::Planner => include_str!("prompts/planner.txt"),
        AgentRole::Dockeeper => include_str!("prompts/dockeeper.txt"),
    }
}

/// MCP tool whitelist for this role.
/// Note: not yet enforced in MCP config — reserved for future tool filtering.
#[must_use]
pub const fn allowed_tools(role: AgentRole) -> &'static [&'static str] {
    match role {
        AgentRole::Coder => &[
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
        AgentRole::Reviewer => &[
            "get_entity",
            "list_entities",
            "get_inbox",
            "send_message",
            "context_query",
            "list_reservations",
        ],
        AgentRole::Planner => &[
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
        AgentRole::Dockeeper => &[
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_roundtrip() {
        for role in AgentRole::ALL {
            let parsed: AgentRole = role.as_str().parse().unwrap();
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
            assert!(!system_prompt(*role).is_empty(), "{role} prompt is empty");
        }
    }

    #[test]
    fn test_allowed_tools_non_empty() {
        for role in AgentRole::ALL {
            assert!(
                !allowed_tools(*role).is_empty(),
                "{role} has no allowed tools"
            );
        }
    }
}
