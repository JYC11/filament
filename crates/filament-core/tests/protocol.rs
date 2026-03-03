use filament_core::error::StructuredError;
use filament_core::protocol::{Method, Request, Response};

#[test]
fn request_roundtrip() {
    let req = Request {
        id: "req-1".to_string(),
        method: Method::CreateEntity,
        params: serde_json::json!({"name": "test"}),
    };
    let json = serde_json::to_string(&req).unwrap();
    let parsed: Request = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.id, "req-1");
    assert!(matches!(parsed.method, Method::CreateEntity));
}

#[test]
fn success_response_roundtrip() {
    let resp = Response::success("req-1".to_string(), serde_json::json!({"id": "e-1"}));
    let json = serde_json::to_string(&resp).unwrap();
    let parsed: Response = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.id, "req-1");
    assert!(parsed.result.is_some());
    assert!(parsed.error.is_none());
}

#[test]
fn error_response_roundtrip() {
    let err = StructuredError {
        code: "ENTITY_NOT_FOUND".to_string(),
        message: "Entity not found: x".to_string(),
        hint: Some("check the ID".to_string()),
        retryable: false,
    };
    let resp = Response::error("req-2".to_string(), err);
    let json = serde_json::to_string(&resp).unwrap();
    let parsed: Response = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.id, "req-2");
    assert!(parsed.result.is_none());
    assert!(parsed.error.is_some());
    assert_eq!(parsed.error.unwrap().code, "ENTITY_NOT_FOUND");
}

#[test]
fn method_enum_all_variants_serialize() {
    let methods = [
        Method::CreateEntity,
        Method::GetEntity,
        Method::ListEntities,
        Method::UpdateEntityStatus,
        Method::DeleteEntity,
        Method::CreateRelation,
        Method::ListRelations,
        Method::DeleteRelation,
        Method::SendMessage,
        Method::GetInbox,
        Method::MarkMessageRead,
        Method::AcquireReservation,
        Method::ReleaseReservation,
        Method::ExpireStaleReservations,
        Method::CreateAgentRun,
        Method::FinishAgentRun,
        Method::ListRunningAgents,
        Method::ReadyTasks,
        Method::CriticalPath,
        Method::ImpactScore,
        Method::ContextQuery,
        Method::CheckCycle,
        Method::GetEntityEvents,
    ];
    for method in &methods {
        let json = serde_json::to_string(method).unwrap();
        let parsed: Method = serde_json::from_str(&json).unwrap();
        assert_eq!(
            serde_json::to_string(&parsed).unwrap(),
            json
        );
    }
}
