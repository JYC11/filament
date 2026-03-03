#![allow(dead_code)]

use filament_core::models::{
    EntityId, EntityType, MessageType, RelationType, ValidCreateEntityRequest,
    ValidCreateRelationRequest, ValidSendMessageRequest,
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
        name: "Test task".to_string(),
        entity_type: EntityType::Task,
        summary: "A test task".to_string(),
        key_facts: serde_json::json!({}),
        content_path: None,
        priority: 2,
    }
}

pub fn task_req(name: &str, priority: i32) -> ValidCreateEntityRequest {
    ValidCreateEntityRequest {
        name: name.to_string(),
        entity_type: EntityType::Task,
        summary: format!("Summary of {name}"),
        key_facts: serde_json::json!({}),
        content_path: None,
        priority,
    }
}

pub fn blocks_req(source: &str, target: &str) -> ValidCreateRelationRequest {
    ValidCreateRelationRequest {
        source_id: EntityId::from(source),
        target_id: EntityId::from(target),
        relation_type: RelationType::Blocks,
        weight: 1.0,
        summary: String::new(),
        metadata: serde_json::json!({}),
    }
}

pub fn sample_message_req() -> ValidSendMessageRequest {
    ValidSendMessageRequest {
        from_agent: "agent-a".to_string(),
        to_agent: "agent-b".to_string(),
        body: "hello".to_string(),
        msg_type: MessageType::Text,
        in_reply_to: None,
        task_id: None,
    }
}
