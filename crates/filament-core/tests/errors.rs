use filament_core::error::{FilamentError, StructuredError};

#[test]
fn error_codes_are_stable() {
    let cases: Vec<(FilamentError, &str)> = vec![
        (
            FilamentError::EntityNotFound {
                id: "x".to_string(),
            },
            "ENTITY_NOT_FOUND",
        ),
        (
            FilamentError::RelationNotFound {
                source_id: "a".to_string(),
                target_id: "b".to_string(),
            },
            "RELATION_NOT_FOUND",
        ),
        (
            FilamentError::CycleDetected {
                path: "a->b".to_string(),
            },
            "CYCLE_DETECTED",
        ),
        (
            FilamentError::FileReserved {
                agent: "a".to_string(),
                glob: "*.rs".to_string(),
            },
            "FILE_RESERVED",
        ),
        (FilamentError::ReservationExpired, "RESERVATION_EXPIRED"),
        (
            FilamentError::Validation("bad".to_string()),
            "VALIDATION_ERROR",
        ),
        (FilamentError::Protocol("bad".to_string()), "PROTOCOL_ERROR"),
    ];

    for (err, expected_code) in &cases {
        assert_eq!(err.error_code(), *expected_code);
    }
}

#[test]
fn retryable_only_for_db_and_io() {
    assert!(!FilamentError::EntityNotFound {
        id: "x".to_string()
    }
    .is_retryable());
    assert!(!FilamentError::Validation("bad".to_string()).is_retryable());
    // Database and IO errors are retryable — but we can't easily construct them in a unit test.
}

#[test]
fn hints_populated_for_key_errors() {
    let err = FilamentError::EntityNotFound {
        id: "test".to_string(),
    };
    assert!(err.hint().is_some());
    assert!(err.hint().unwrap().contains("filament entity list"));

    let err = FilamentError::CycleDetected {
        path: "a->b".to_string(),
    };
    assert!(err.hint().is_some());

    let err = FilamentError::Protocol("x".to_string());
    assert!(err.hint().is_none());
}

#[test]
fn exit_codes_categorized() {
    assert_eq!(
        FilamentError::EntityNotFound {
            id: "x".to_string()
        }
        .exit_code(),
        3
    );
    assert_eq!(FilamentError::Validation("x".to_string()).exit_code(), 4);
    assert_eq!(
        FilamentError::CycleDetected {
            path: "x".to_string()
        }
        .exit_code(),
        5
    );
    assert_eq!(FilamentError::ReservationExpired.exit_code(), 6);
}

#[test]
fn structured_error_json_format() {
    let err = FilamentError::EntityNotFound {
        id: "test-id".to_string(),
    };
    let structured = StructuredError::from(&err);
    let json = serde_json::to_value(&structured).unwrap();

    assert_eq!(json["code"], "ENTITY_NOT_FOUND");
    assert!(json["message"].as_str().unwrap().contains("test-id"));
    assert!(json["hint"].is_string());
    assert_eq!(json["retryable"], false);
}
