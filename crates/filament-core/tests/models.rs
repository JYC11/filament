use filament_core::error::FilamentError;
use filament_core::models::*;

// ---------------------------------------------------------------------------
// typed_id! tests
// ---------------------------------------------------------------------------

#[test]
fn entity_id_uniqueness() {
    let a = EntityId::new();
    let b = EntityId::new();
    assert_ne!(a, b);
}

#[test]
fn entity_id_display_fromstr_roundtrip() {
    let id = EntityId::new();
    let s = id.to_string();
    let parsed: EntityId = s.parse().unwrap();
    assert_eq!(id, parsed);
}

#[test]
fn entity_id_from_string() {
    let id = EntityId::from("test-123".to_string());
    assert_eq!(id.as_str(), "test-123");
}

#[test]
fn entity_id_from_str() {
    let id = EntityId::from("test-456");
    assert_eq!(id.as_str(), "test-456");
}

#[test]
fn relation_id_uniqueness() {
    let a = RelationId::new();
    let b = RelationId::new();
    assert_ne!(a, b);
}

// ---------------------------------------------------------------------------
// Enum serde round-trips
// ---------------------------------------------------------------------------

#[test]
fn entity_type_serde_roundtrip() {
    let val = EntityType::Task;
    let json = serde_json::to_string(&val).unwrap();
    assert_eq!(json, "\"task\"");
    let parsed: EntityType = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, val);
}

#[test]
fn relation_type_serde_roundtrip() {
    let val = RelationType::DependsOn;
    let json = serde_json::to_string(&val).unwrap();
    assert_eq!(json, "\"depends_on\"");
    let parsed: RelationType = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, val);
}

#[test]
fn entity_status_serde_roundtrip() {
    let val = EntityStatus::InProgress;
    let json = serde_json::to_string(&val).unwrap();
    assert_eq!(json, "\"in_progress\"");
    let parsed: EntityStatus = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, val);
}

#[test]
fn agent_status_all_variants_serialize() {
    let variants = [
        (AgentStatus::Running, "\"running\""),
        (AgentStatus::Completed, "\"completed\""),
        (AgentStatus::Blocked, "\"blocked\""),
        (AgentStatus::Failed, "\"failed\""),
        (AgentStatus::NeedsInput, "\"needs_input\""),
    ];
    for (variant, expected) in variants {
        assert_eq!(serde_json::to_string(&variant).unwrap(), expected);
    }
}

// ---------------------------------------------------------------------------
// AgentResult deserialization
// ---------------------------------------------------------------------------

#[test]
fn agent_result_deserialize() {
    let json = r#"{
        "status": "completed",
        "task_id": "task-1",
        "summary": "done",
        "artifacts": ["file.rs"],
        "messages": [{"to_agent": "orchestrator", "body": "finished", "msg_type": "text"}],
        "blockers": [],
        "questions": []
    }"#;
    let result: AgentResult = serde_json::from_str(json).unwrap();
    assert_eq!(result.status, AgentStatus::Completed);
    assert_eq!(result.artifacts.len(), 1);
    assert_eq!(result.messages.len(), 1);
    assert_eq!(result.messages[0].to_agent, "orchestrator");
}

// ---------------------------------------------------------------------------
// DTO validation (TryFrom)
// ---------------------------------------------------------------------------

#[test]
fn create_entity_valid() {
    let req = CreateEntityRequest {
        name: "My Task".to_string(),
        entity_type: "task".to_string(),
        summary: None,
        key_facts: None,
        content_path: None,
        priority: None,
    };
    let valid = ValidCreateEntityRequest::try_from(req).unwrap();
    assert_eq!(valid.name, "My Task");
    assert_eq!(valid.entity_type, EntityType::Task);
    assert_eq!(valid.priority, 2); // default
}

#[test]
fn create_entity_empty_name_rejected() {
    let req = CreateEntityRequest {
        name: "  ".to_string(),
        entity_type: "task".to_string(),
        summary: None,
        key_facts: None,
        content_path: None,
        priority: None,
    };
    let err = ValidCreateEntityRequest::try_from(req).unwrap_err();
    assert!(matches!(err, FilamentError::Validation(_)));
}

#[test]
fn create_entity_invalid_type_rejected() {
    let req = CreateEntityRequest {
        name: "test".to_string(),
        entity_type: "unknown".to_string(),
        summary: None,
        key_facts: None,
        content_path: None,
        priority: None,
    };
    let err = ValidCreateEntityRequest::try_from(req).unwrap_err();
    assert!(matches!(err, FilamentError::Validation(_)));
}

#[test]
fn create_entity_bad_priority_rejected() {
    let req = CreateEntityRequest {
        name: "test".to_string(),
        entity_type: "task".to_string(),
        summary: None,
        key_facts: None,
        content_path: None,
        priority: Some(5),
    };
    let err = ValidCreateEntityRequest::try_from(req).unwrap_err();
    assert!(matches!(err, FilamentError::Validation(_)));
}

#[test]
fn create_relation_self_loop_rejected() {
    let req = CreateRelationRequest {
        source_id: "abc".to_string(),
        target_id: "abc".to_string(),
        relation_type: "blocks".to_string(),
        weight: None,
        summary: None,
        metadata: None,
    };
    let err = ValidCreateRelationRequest::try_from(req).unwrap_err();
    assert!(matches!(err, FilamentError::Validation(_)));
}

#[test]
fn send_message_empty_body_rejected() {
    let req = SendMessageRequest {
        from_agent: "a".to_string(),
        to_agent: "b".to_string(),
        body: " ".to_string(),
        msg_type: None,
        in_reply_to: None,
        task_id: None,
    };
    let err = ValidSendMessageRequest::try_from(req).unwrap_err();
    assert!(matches!(err, FilamentError::Validation(_)));
}
