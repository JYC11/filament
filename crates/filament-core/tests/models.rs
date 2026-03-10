use filament_core::dto::*;
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
    let req = CreateEntityRequest::from_parts(
        EntityType::Task,
        "My Task".to_string(),
        None,
        None,
        None,
        None,
    )
    .unwrap();
    let valid = ValidCreateEntityRequest::try_from(req).unwrap();
    assert_eq!(valid.name, "My Task");
    assert_eq!(valid.entity_type, EntityType::Task);
    assert_eq!(valid.priority, Priority::DEFAULT);
}

#[test]
fn create_entity_empty_name_rejected() {
    let req =
        CreateEntityRequest::from_parts(EntityType::Task, "  ".to_string(), None, None, None, None)
            .unwrap();
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
        content: None,
        status: EntityStatus::Open,
        priority: Priority::DEFAULT,
        version: 0,
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

// ---------------------------------------------------------------------------
// Value type edge cases
// ---------------------------------------------------------------------------

#[test]
fn slug_try_from_trims_whitespace() {
    let s = Slug::try_from("  ab12cd34  ".to_string()).unwrap();
    assert_eq!(s.as_str(), "ab12cd34");
}

#[test]
fn slug_try_from_trims_tabs_and_newlines() {
    // Slug::try_from trims all whitespace (including tabs/newlines),
    // so "\tab12cd34\n" becomes "ab12cd34" which is valid.
    let s = Slug::try_from("\tab12cd34\n".to_string());
    assert!(
        s.is_ok(),
        "tabs/newlines are trimmed, leaving a valid 8-char slug"
    );
    assert_eq!(s.unwrap().as_str(), "ab12cd34");
}

#[test]
fn slug_try_from_rejects_empty() {
    assert!(Slug::try_from(String::new()).is_err());
}

#[test]
fn priority_display() {
    assert_eq!(Priority::new(0).unwrap().to_string(), "0");
    assert_eq!(Priority::new(4).unwrap().to_string(), "4");
}

#[test]
fn weight_zero_is_valid() {
    let w = Weight::new(0.0).unwrap();
    assert!((w.value() - 0.0).abs() < f64::EPSILON);
}

#[test]
fn weight_rejects_negative_zero() {
    // -0.0 is NOT negative in IEEE 754 (it equals 0.0), so should be valid
    let w = Weight::new(-0.0);
    assert!(w.is_ok(), "-0.0 should be valid (IEEE 754: -0.0 == 0.0)");
}

#[test]
fn budget_pct_negative_zero_valid() {
    // -0.0 == 0.0 in IEEE 754, should be in range
    let b = BudgetPct::new(-0.0);
    assert!(b.is_ok(), "-0.0 should be valid");
}

#[test]
fn budget_pct_display_formatting() {
    assert_eq!(BudgetPct::new(0.0).unwrap().to_string(), "0%");
    assert_eq!(BudgetPct::new(0.5).unwrap().to_string(), "50%");
    assert_eq!(BudgetPct::new(1.0).unwrap().to_string(), "100%");
}

#[test]
fn budget_pct_serde_roundtrip() {
    let b = BudgetPct::new(0.75).unwrap();
    let json = serde_json::to_string(&b).unwrap();
    assert_eq!(json, "0.75");
    let parsed: BudgetPct = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, b);
}

#[test]
fn budget_pct_serde_rejects_out_of_range() {
    let result: Result<BudgetPct, _> = serde_json::from_str("1.5");
    assert!(result.is_err());
    let result: Result<BudgetPct, _> = serde_json::from_str("-0.1");
    assert!(result.is_err());
}

#[test]
fn non_empty_string_rejects_tabs_and_newlines_only() {
    assert!(NonEmptyString::new("\t\n\r").is_err());
}

#[test]
fn non_empty_string_preserves_inner_whitespace() {
    let s = NonEmptyString::new("  hello   world  ").unwrap();
    assert_eq!(s.as_str(), "hello   world");
}

#[test]
fn ttl_seconds_u32_max_valid() {
    let t = TtlSeconds::new(u32::MAX).unwrap();
    assert_eq!(t.value(), u32::MAX);
}

#[test]
fn ttl_seconds_display() {
    assert_eq!(TtlSeconds::new(60).unwrap().to_string(), "60s");
}

#[test]
fn ttl_seconds_as_duration() {
    let t = TtlSeconds::new(3600).unwrap();
    let d = t.as_duration();
    assert_eq!(d.num_seconds(), 3600);
}

// ---------------------------------------------------------------------------
// Enum FromStr edge cases
// ---------------------------------------------------------------------------

#[test]
fn entity_type_fromstr_case_insensitive() {
    use std::str::FromStr;
    assert_eq!(EntityType::from_str("TASK").unwrap(), EntityType::Task);
    assert_eq!(EntityType::from_str("Task").unwrap(), EntityType::Task);
    assert_eq!(EntityType::from_str("tAsK").unwrap(), EntityType::Task);
    assert_eq!(EntityType::from_str("LESSON").unwrap(), EntityType::Lesson);
}

#[test]
fn entity_type_fromstr_rejects_with_spaces() {
    use std::str::FromStr;
    assert!(EntityType::from_str("  task  ").is_err());
    assert!(EntityType::from_str(" ").is_err());
    assert!(EntityType::from_str("").is_err());
}

#[test]
fn entity_type_fromstr_rejects_substring() {
    use std::str::FromStr;
    assert!(EntityType::from_str("tas").is_err());
    assert!(EntityType::from_str("taskk").is_err());
}

#[test]
fn relation_type_fromstr_case_insensitive() {
    use std::str::FromStr;
    assert_eq!(
        RelationType::from_str("BLOCKS").unwrap(),
        RelationType::Blocks
    );
    assert_eq!(
        RelationType::from_str("Depends_On").unwrap(),
        RelationType::DependsOn
    );
}

#[test]
fn entity_status_fromstr_case_insensitive() {
    use std::str::FromStr;
    assert_eq!(
        EntityStatus::from_str("IN_PROGRESS").unwrap(),
        EntityStatus::InProgress
    );
    assert_eq!(
        EntityStatus::from_str("CLOSED").unwrap(),
        EntityStatus::Closed
    );
}

#[test]
fn event_type_fromstr_all_variants() {
    use std::str::FromStr;
    let variants = [
        ("entity_created", EventType::EntityCreated),
        ("entity_updated", EventType::EntityUpdated),
        ("entity_deleted", EventType::EntityDeleted),
        ("status_change", EventType::StatusChange),
        ("relation_created", EventType::RelationCreated),
        ("relation_deleted", EventType::RelationDeleted),
        ("message_sent", EventType::MessageSent),
        ("message_read", EventType::MessageRead),
        ("reservation_acquired", EventType::ReservationAcquired),
        ("reservation_released", EventType::ReservationReleased),
        ("agent_started", EventType::AgentStarted),
        ("agent_finished", EventType::AgentFinished),
    ];
    for (s, expected) in variants {
        assert_eq!(EventType::from_str(s).unwrap(), expected, "failed for: {s}");
    }
}

#[test]
fn agent_role_fromstr_case_insensitive() {
    use std::str::FromStr;
    assert_eq!(AgentRole::from_str("CODER").unwrap(), AgentRole::Coder);
    assert_eq!(
        AgentRole::from_str("Reviewer").unwrap(),
        AgentRole::Reviewer
    );
}

#[test]
fn reservation_mode_fromstr() {
    use std::str::FromStr;
    assert_eq!(
        ReservationMode::from_str("exclusive").unwrap(),
        ReservationMode::Exclusive
    );
    assert_eq!(
        ReservationMode::from_str("shared").unwrap(),
        ReservationMode::Shared
    );
    // ReservationMode does NOT use impl_enum_str, so case-sensitivity is exact
    assert!(ReservationMode::from_str("EXCLUSIVE").is_err());
}

// ---------------------------------------------------------------------------
// Entity ADT edge cases
// ---------------------------------------------------------------------------

#[test]
fn entity_adt_lesson_variant() {
    let e = Entity::Lesson(sample_common());
    assert_eq!(e.entity_type(), EntityType::Lesson);
    assert!(matches!(e, Entity::Lesson(_)));
}

#[test]
fn entity_into_task_type_mismatch_preserves_info() {
    let e = Entity::Agent(sample_common());
    let err = e.into_task().unwrap_err();
    match err {
        FilamentError::TypeMismatch {
            expected, actual, ..
        } => {
            assert_eq!(expected, EntityType::Task);
            assert_eq!(actual, EntityType::Agent);
        }
        other => panic!("expected TypeMismatch, got: {other:?}"),
    }
}

#[test]
fn entity_into_lesson_type_mismatch() {
    let e = Entity::Doc(sample_common());
    let err = e.into_lesson().unwrap_err();
    assert!(matches!(err, FilamentError::TypeMismatch { .. }));
}

#[test]
fn entity_into_agent_success() {
    let e = Entity::Agent(sample_common());
    let c = e.into_agent().unwrap();
    assert_eq!(c.name.as_str(), "test-entity");
}

// ---------------------------------------------------------------------------
// LessonFields edge cases
// ---------------------------------------------------------------------------

#[test]
fn lesson_fields_from_key_facts_missing_problem() {
    let kf = serde_json::json!({"solution": "fix", "learned": "stuff"});
    assert!(LessonFields::from_key_facts(&kf).is_none());
}

#[test]
fn lesson_fields_from_key_facts_missing_solution() {
    let kf = serde_json::json!({"problem": "bug", "learned": "stuff"});
    assert!(LessonFields::from_key_facts(&kf).is_none());
}

#[test]
fn lesson_fields_from_key_facts_missing_learned() {
    let kf = serde_json::json!({"problem": "bug", "solution": "fix"});
    assert!(LessonFields::from_key_facts(&kf).is_none());
}

#[test]
fn lesson_fields_from_key_facts_null_values() {
    let kf = serde_json::json!({"problem": null, "solution": "fix", "learned": "stuff"});
    assert!(LessonFields::from_key_facts(&kf).is_none());
}

#[test]
fn lesson_fields_from_key_facts_integer_values() {
    let kf = serde_json::json!({"problem": 123, "solution": "fix", "learned": "stuff"});
    assert!(LessonFields::from_key_facts(&kf).is_none());
}

#[test]
fn lesson_fields_from_key_facts_empty_object() {
    let kf = serde_json::json!({});
    assert!(LessonFields::from_key_facts(&kf).is_none());
}

#[test]
fn lesson_fields_from_key_facts_not_object() {
    let kf = serde_json::json!("just a string");
    assert!(LessonFields::from_key_facts(&kf).is_none());
}

#[test]
fn lesson_fields_from_key_facts_array() {
    let kf = serde_json::json!([1, 2, 3]);
    assert!(LessonFields::from_key_facts(&kf).is_none());
}

#[test]
fn lesson_fields_pattern_optional() {
    let kf = serde_json::json!({
        "problem": "bug",
        "solution": "fix",
        "learned": "insight"
    });
    let fields = LessonFields::from_key_facts(&kf).unwrap();
    assert!(fields.pattern.is_none());
}

#[test]
fn lesson_fields_roundtrip() {
    let fields = LessonFields {
        problem: "some bug".to_string(),
        solution: "the fix".to_string(),
        pattern: Some("sqlx".to_string()),
        learned: "always check".to_string(),
    };
    let kf = fields.to_key_facts();
    let back = LessonFields::from_key_facts(&kf).unwrap();
    assert_eq!(back.problem, "some bug");
    assert_eq!(back.solution, "the fix");
    assert_eq!(back.pattern.as_deref(), Some("sqlx"));
    assert_eq!(back.learned, "always check");
}

#[test]
fn lesson_fields_from_entity_non_lesson() {
    let e = Entity::Task(sample_common());
    assert!(LessonFields::from_entity(&e).is_none());
}

// ---------------------------------------------------------------------------
// DTO edge cases
// ---------------------------------------------------------------------------

#[test]
fn entity_changeset_is_empty_all_none() {
    let common = ChangesetCommon {
        name: None,
        summary: None,
        status: None,
        priority: None,
        key_facts: None,
        expected_version: 0,
    };
    let cs = EntityChangeset::for_type(EntityType::Task, common, None);
    assert!(cs.is_empty());
    assert!(cs.changed_field_names().is_empty());
}

#[test]
fn entity_changeset_not_empty_with_summary() {
    let common = ChangesetCommon {
        name: None,
        summary: Some("updated".to_string()),
        status: None,
        priority: None,
        key_facts: None,
        expected_version: 1,
    };
    let cs = EntityChangeset::for_type(EntityType::Task, common, None);
    assert!(!cs.is_empty());
    assert_eq!(cs.changed_field_names(), vec!["summary"]);
}

#[test]
fn entity_changeset_multiple_fields() {
    let common = ChangesetCommon {
        name: Some(NonEmptyString::new("new name").unwrap()),
        summary: None,
        status: Some(EntityStatus::Closed),
        priority: Some(Priority::new(0).unwrap()),
        key_facts: None,
        expected_version: 2,
    };
    let cs = EntityChangeset::for_type(EntityType::Task, common, None);
    let fields = cs.changed_field_names();
    assert!(fields.contains(&"name"));
    assert!(fields.contains(&"status"));
    assert!(fields.contains(&"priority"));
    assert_eq!(fields.len(), 3);
}

#[test]
fn send_message_empty_from_rejected() {
    let req = SendMessageRequest {
        from_agent: "  ".to_string(),
        to_agent: "b".to_string(),
        body: "hello".to_string(),
        msg_type: None,
        in_reply_to: None,
        task_id: None,
    };
    let err = ValidSendMessageRequest::try_from(req).unwrap_err();
    assert!(matches!(err, FilamentError::Validation(_)));
}

#[test]
fn send_message_empty_to_rejected() {
    let req = SendMessageRequest {
        from_agent: "a".to_string(),
        to_agent: "  ".to_string(),
        body: "hello".to_string(),
        msg_type: None,
        in_reply_to: None,
        task_id: None,
    };
    let err = ValidSendMessageRequest::try_from(req).unwrap_err();
    assert!(matches!(err, FilamentError::Validation(_)));
}

#[test]
fn send_message_defaults_to_text_type() {
    let req = SendMessageRequest {
        from_agent: "a".to_string(),
        to_agent: "b".to_string(),
        body: "hello".to_string(),
        msg_type: None,
        in_reply_to: None,
        task_id: None,
    };
    let valid = ValidSendMessageRequest::try_from(req).unwrap();
    assert_eq!(valid.msg_type, MessageType::Text);
}

#[test]
fn create_entity_with_facts_and_content() {
    let req = CreateEntityRequest::from_parts(
        EntityType::Doc,
        "Doc Entity".to_string(),
        Some("A doc".to_string()),
        Some(Priority::new(1).unwrap()),
        Some(serde_json::json!({"key": "val"})),
        Some("/path/to/file.md".to_string()),
    )
    .unwrap();
    let valid = ValidCreateEntityRequest::try_from(req).unwrap();
    assert_eq!(valid.name, "Doc Entity");
    assert_eq!(valid.priority.value(), 1);
    assert_eq!(valid.content_path.as_deref(), Some("/path/to/file.md"));
    assert_eq!(valid.key_facts["key"], "val");
}
