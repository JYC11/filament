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
// Slug tests
// ---------------------------------------------------------------------------

#[test]
fn slug_new_generates_valid_8char() {
    let s = Slug::new();
    assert_eq!(s.as_str().len(), 8);
    assert!(s
        .as_str()
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
}

#[test]
fn slug_uniqueness() {
    let a = Slug::new();
    let b = Slug::new();
    assert_ne!(a, b);
}

#[test]
fn slug_try_from_valid() {
    let s = Slug::try_from("ab12cd34".to_string()).unwrap();
    assert_eq!(s.as_str(), "ab12cd34");
}

#[test]
fn slug_try_from_rejects_too_short() {
    assert!(Slug::try_from("abc".to_string()).is_err());
}

#[test]
fn slug_try_from_rejects_too_long() {
    assert!(Slug::try_from("abcdefghi".to_string()).is_err());
}

#[test]
fn slug_try_from_rejects_uppercase() {
    assert!(Slug::try_from("ABCDEFGH".to_string()).is_err());
}

#[test]
fn slug_try_from_rejects_special_chars() {
    assert!(Slug::try_from("ab-cd_ef".to_string()).is_err());
}

#[test]
fn slug_display_roundtrip() {
    let s = Slug::new();
    let displayed = s.to_string();
    let parsed: Slug = displayed.parse().unwrap();
    assert_eq!(s, parsed);
}

#[test]
fn slug_serde_roundtrip() {
    let s = Slug::new();
    let json = serde_json::to_string(&s).unwrap();
    let parsed: Slug = serde_json::from_str(&json).unwrap();
    assert_eq!(s, parsed);
}

// ---------------------------------------------------------------------------
// Value type tests
// ---------------------------------------------------------------------------

#[test]
fn priority_valid_range() {
    for i in 0..=4u8 {
        assert!(Priority::new(i).is_ok());
    }
    assert!(Priority::new(5).is_err());
    assert!(Priority::new(255).is_err());
}

#[test]
fn priority_default_is_2() {
    assert_eq!(Priority::DEFAULT.value(), 2);
}

#[test]
fn priority_ordering() {
    let p0 = Priority::new(0).unwrap();
    let p4 = Priority::new(4).unwrap();
    assert!(p0 < p4);
}

#[test]
fn priority_serde_roundtrip() {
    let p = Priority::new(3).unwrap();
    let json = serde_json::to_string(&p).unwrap();
    assert_eq!(json, "3");
    let parsed: Priority = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, p);
}

#[test]
fn priority_serde_rejects_invalid() {
    let result: Result<Priority, _> = serde_json::from_str("5");
    assert!(result.is_err());
}

#[test]
fn weight_valid() {
    assert!(Weight::new(0.0).is_ok());
    assert!(Weight::new(1.0).is_ok());
    assert!(Weight::new(100.0).is_ok());
}

#[test]
fn weight_rejects_invalid() {
    assert!(Weight::new(-1.0).is_err());
    assert!(Weight::new(f64::NAN).is_err());
    assert!(Weight::new(f64::INFINITY).is_err());
    assert!(Weight::new(f64::NEG_INFINITY).is_err());
}

#[test]
fn weight_serde_roundtrip() {
    let w = Weight::new(2.5).unwrap();
    let json = serde_json::to_string(&w).unwrap();
    assert_eq!(json, "2.5");
    let parsed: Weight = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, w);
}

#[test]
fn budget_pct_valid_range() {
    assert!(BudgetPct::new(0.0).is_ok());
    assert!(BudgetPct::new(0.5).is_ok());
    assert!(BudgetPct::new(1.0).is_ok());
}

#[test]
fn budget_pct_rejects_out_of_range() {
    assert!(BudgetPct::new(-0.1).is_err());
    assert!(BudgetPct::new(1.1).is_err());
    assert!(BudgetPct::new(f64::NAN).is_err());
}

#[test]
fn non_empty_string_rejects_empty() {
    assert!(NonEmptyString::new("").is_err());
    assert!(NonEmptyString::new("   ").is_err());
}

#[test]
fn non_empty_string_trims() {
    let s = NonEmptyString::new("  hello  ").unwrap();
    assert_eq!(s.as_str(), "hello");
}

#[test]
fn non_empty_string_partial_eq_str() {
    let s = NonEmptyString::new("hello").unwrap();
    assert_eq!(s, "hello");
    assert!(s != "world");
}

#[test]
fn non_empty_string_serde_roundtrip() {
    let s = NonEmptyString::new("test").unwrap();
    let json = serde_json::to_string(&s).unwrap();
    assert_eq!(json, "\"test\"");
    let parsed: NonEmptyString = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, s);
}

#[test]
fn non_empty_string_serde_rejects_empty() {
    let result: Result<NonEmptyString, _> = serde_json::from_str("\"\"");
    assert!(result.is_err());
    let result: Result<NonEmptyString, _> = serde_json::from_str("\"   \"");
    assert!(result.is_err());
}

#[test]
fn ttl_seconds_valid() {
    assert!(TtlSeconds::new(1).is_ok());
    assert!(TtlSeconds::new(3600).is_ok());
}

#[test]
fn ttl_seconds_rejects_zero() {
    assert!(TtlSeconds::new(0).is_err());
}

// ---------------------------------------------------------------------------
// EventType tests
// ---------------------------------------------------------------------------

#[test]
fn event_type_serde_roundtrip() {
    let val = EventType::StatusChange;
    let json = serde_json::to_string(&val).unwrap();
    assert_eq!(json, "\"status_change\"");
    let parsed: EventType = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, val);
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
        entity_type: EntityType::Task,
        summary: None,
        key_facts: None,
        content_path: None,
        priority: None,
    };
    let valid = ValidCreateEntityRequest::try_from(req).unwrap();
    assert_eq!(valid.name, "My Task");
    assert_eq!(valid.entity_type, EntityType::Task);
    assert_eq!(valid.priority, Priority::DEFAULT);
}

#[test]
fn create_entity_empty_name_rejected() {
    let req = CreateEntityRequest {
        name: "  ".to_string(),
        entity_type: EntityType::Task,
        summary: None,
        key_facts: None,
        content_path: None,
        priority: None,
    };
    let err = ValidCreateEntityRequest::try_from(req).unwrap_err();
    assert!(matches!(err, FilamentError::Validation(_)));
}

#[test]
fn create_entity_invalid_type_rejected_by_serde() {
    // Invalid entity types are now rejected at deserialization, not TryFrom
    let json = r#"{"name":"test","entity_type":"unknown","summary":null,"key_facts":null,"content_path":null,"priority":null}"#;
    let result: Result<CreateEntityRequest, _> = serde_json::from_str(json);
    assert!(result.is_err());
}

#[test]
fn create_entity_bad_priority_rejected_by_serde() {
    // Invalid priorities are now rejected at deserialization, not TryFrom
    let json = r#"{"name":"test","entity_type":"task","summary":null,"key_facts":null,"content_path":null,"priority":5}"#;
    let result: Result<CreateEntityRequest, _> = serde_json::from_str(json);
    assert!(result.is_err());
}

#[test]
fn create_relation_self_loop_rejected() {
    let req = CreateRelationRequest {
        source_id: "abc".to_string(),
        target_id: "abc".to_string(),
        relation_type: RelationType::Blocks,
        weight: None,
        summary: None,
        metadata: None,
    };
    let err = ValidCreateRelationRequest::try_from(req).unwrap_err();
    assert!(matches!(err, FilamentError::Validation(_)));
}

#[test]
fn create_relation_trims_whitespace_ids() {
    let req = CreateRelationRequest {
        source_id: "  abc  ".to_string(),
        target_id: "  def  ".to_string(),
        relation_type: RelationType::Blocks,
        weight: None,
        summary: None,
        metadata: None,
    };
    let valid = ValidCreateRelationRequest::try_from(req).unwrap();
    assert_eq!(valid.source_id.as_str(), "abc");
    assert_eq!(valid.target_id.as_str(), "def");
}

#[test]
fn create_relation_whitespace_only_ids_rejected() {
    let req = CreateRelationRequest {
        source_id: "   ".to_string(),
        target_id: "b".to_string(),
        relation_type: RelationType::Blocks,
        weight: None,
        summary: None,
        metadata: None,
    };
    let err = ValidCreateRelationRequest::try_from(req).unwrap_err();
    assert!(matches!(err, FilamentError::Validation(_)));
}

#[test]
fn create_relation_self_loop_after_trim_rejected() {
    let req = CreateRelationRequest {
        source_id: " abc ".to_string(),
        target_id: "abc".to_string(),
        relation_type: RelationType::Blocks,
        weight: None,
        summary: None,
        metadata: None,
    };
    let err = ValidCreateRelationRequest::try_from(req).unwrap_err();
    assert!(matches!(err, FilamentError::Validation(_)));
}

#[test]
fn create_relation_bad_weight_rejected() {
    let req = CreateRelationRequest {
        source_id: "a".to_string(),
        target_id: "b".to_string(),
        relation_type: RelationType::Blocks,
        weight: Some(-1.0),
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

// ---------------------------------------------------------------------------
// AgentMessage validation via NonEmptyString
// ---------------------------------------------------------------------------

#[test]
fn agent_message_rejects_empty_to_agent() {
    let json = r#"{"to_agent": "", "body": "hello", "msg_type": "text"}"#;
    let result: Result<AgentMessage, _> = serde_json::from_str(json);
    assert!(result.is_err());
}

#[test]
fn agent_message_rejects_empty_body() {
    let json = r#"{"to_agent": "orchestrator", "body": "   ", "msg_type": "text"}"#;
    let result: Result<AgentMessage, _> = serde_json::from_str(json);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Entity ADT tests
// ---------------------------------------------------------------------------

fn sample_common() -> EntityCommon {
    EntityCommon {
        id: EntityId::new(),
        slug: Slug::new(),
        name: NonEmptyString::new("test-entity").unwrap(),
        summary: "A test entity".to_string(),
        key_facts: serde_json::json!({}),
        content_path: None,
        content_hash: None,
        status: EntityStatus::Open,
        priority: Priority::DEFAULT,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

#[test]
fn entity_adt_task_variant() {
    let e = Entity::Task(sample_common());
    assert_eq!(e.entity_type(), EntityType::Task);
    assert!(matches!(e, Entity::Task(_)));
    assert!(!matches!(e, Entity::Agent(_)));
}

#[test]
fn entity_adt_agent_variant() {
    let e = Entity::Agent(sample_common());
    assert_eq!(e.entity_type(), EntityType::Agent);
    assert!(matches!(e, Entity::Agent(_)));
    assert!(!matches!(e, Entity::Task(_)));
}

#[test]
fn entity_adt_accessors() {
    let common = sample_common();
    let expected_name = common.name.clone();
    let expected_slug = common.slug.clone();
    let e = Entity::Module(common);
    assert_eq!(e.name(), &expected_name);
    assert_eq!(e.slug(), &expected_slug);
    assert_eq!(*e.status(), EntityStatus::Open);
    assert_eq!(e.priority(), Priority::DEFAULT);
    assert_eq!(e.summary(), "A test entity");
}

#[test]
fn entity_adt_serde_roundtrip() {
    let e = Entity::Service(sample_common());
    let json = serde_json::to_string(&e).unwrap();
    assert!(json.contains("\"entity_type\":\"service\""));
    let parsed: Entity = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.entity_type(), EntityType::Service);
    assert_eq!(parsed.name(), e.name());
}

#[test]
fn entity_adt_all_variants_type_check() {
    let variants = vec![
        (Entity::Task(sample_common()), EntityType::Task),
        (Entity::Module(sample_common()), EntityType::Module),
        (Entity::Service(sample_common()), EntityType::Service),
        (Entity::Agent(sample_common()), EntityType::Agent),
        (Entity::Plan(sample_common()), EntityType::Plan),
        (Entity::Doc(sample_common()), EntityType::Doc),
    ];
    for (entity, expected_type) in variants {
        assert_eq!(entity.entity_type(), expected_type);
    }
}
