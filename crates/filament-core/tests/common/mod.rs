#![allow(dead_code)]

use filament_core::dto::{
    ValidCreateEntityRequest, ValidCreateRelationRequest, ValidSendMessageRequest,
};
use filament_core::models::{
    EntityId, EntityType, MessageType, NonEmptyString, Priority, RelationType, Weight,
};
use filament_core::schema::init_test_pool;
use filament_core::store::FilamentStore;

/// Fresh in-memory `SQLite` per test — no shared state.
pub async fn test_db() -> FilamentStore {
    let pool = init_test_pool().await.unwrap();
    FilamentStore::new(pool)
}

pub fn sample_entity_req() -> ValidCreateEntityRequest {
    ValidCreateEntityRequest {
        name: NonEmptyString::new("Test task").unwrap(),
        entity_type: EntityType::Task,
        summary: "A test task".to_string(),
        key_facts: serde_json::json!({}),
        content_path: None,
        priority: Priority::DEFAULT,
    }
}

pub fn task_req(name: &str, priority: u8) -> ValidCreateEntityRequest {
    ValidCreateEntityRequest {
        name: NonEmptyString::new(name).unwrap(),
        entity_type: EntityType::Task,
        summary: format!("Summary of {name}"),
        key_facts: serde_json::json!({}),
        content_path: None,
        priority: Priority::new(priority).unwrap(),
    }
}

pub fn blocks_req(source: &str, target: &str) -> ValidCreateRelationRequest {
    ValidCreateRelationRequest {
        source_id: EntityId::from(source),
        target_id: EntityId::from(target),
        relation_type: RelationType::Blocks,
        weight: Weight::DEFAULT,
        summary: String::new(),
        metadata: serde_json::json!({}),
    }
}

pub fn depends_on_req(source: &str, target: &str) -> ValidCreateRelationRequest {
    ValidCreateRelationRequest {
        source_id: EntityId::from(source),
        target_id: EntityId::from(target),
        relation_type: RelationType::DependsOn,
        weight: Weight::DEFAULT,
        summary: String::new(),
        metadata: serde_json::json!({}),
    }
}

pub fn lesson_req(
    name: &str,
    problem: &str,
    solution: &str,
    learned: &str,
) -> ValidCreateEntityRequest {
    use filament_core::models::LessonFields;
    let fields = LessonFields {
        problem: problem.to_string(),
        solution: solution.to_string(),
        pattern: None,
        learned: learned.to_string(),
    };
    ValidCreateEntityRequest {
        name: NonEmptyString::new(name).unwrap(),
        entity_type: EntityType::Lesson,
        summary: learned.to_string(),
        key_facts: fields.to_key_facts(),
        content_path: None,
        priority: Priority::DEFAULT,
    }
}

pub fn sample_message_req() -> ValidSendMessageRequest {
    ValidSendMessageRequest {
        from_agent: NonEmptyString::new("agent-a").unwrap(),
        to_agent: NonEmptyString::new("agent-b").unwrap(),
        body: NonEmptyString::new("hello").unwrap(),
        msg_type: MessageType::Text,
        in_reply_to: None,
        task_id: None,
    }
}
