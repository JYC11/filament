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
/// Names must match MCP tool names (the `fl_*` names from mcp.rs).
/// Enforced server-side: disallowed tools return an error.
#[must_use]
pub const fn allowed_tools(role: AgentRole) -> &'static [&'static str] {
    match role {
        AgentRole::Coder => &[
            "fl_inspect",
            "fl_list",
            "fl_message_inbox",
            "fl_message_send",
            "fl_message_read",
            "fl_reserve",
            "fl_release",
            "fl_reservations",
            "fl_task_ready",
            "fl_task_close",
            "fl_context",
        ],
        AgentRole::Reviewer => &[
            "fl_inspect",
            "fl_list",
            "fl_message_inbox",
            "fl_message_send",
            "fl_message_read",
            "fl_context",
            "fl_reservations",
        ],
        AgentRole::Planner => &[
            "fl_inspect",
            "fl_list",
            "fl_add",
            "fl_relate",
            "fl_message_send",
            "fl_message_inbox",
            "fl_message_read",
            "fl_task_ready",
            "fl_context",
        ],
        AgentRole::Dockeeper => &[
            "fl_inspect",
            "fl_list",
            "fl_update",
            "fl_message_send",
            "fl_message_inbox",
            "fl_message_read",
            "fl_context",
            "fl_reserve",
            "fl_release",
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

    #[test]
    fn test_allowed_tools_use_fl_prefix() {
        for role in AgentRole::ALL {
            for tool in allowed_tools(*role) {
                assert!(
                    tool.starts_with("fl_"),
                    "{role}: tool '{tool}' must start with 'fl_' to match MCP names"
                );
            }
        }
    }
}
