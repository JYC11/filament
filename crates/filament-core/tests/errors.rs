use filament_core::error::{FilamentError, StructuredError};
use filament_core::models::{EntityType, Slug};

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
                id: "rel-1".to_string(),
            },
            "RELATION_NOT_FOUND",
        ),
        (
            FilamentError::MessageNotFound {
                id: "msg-1".to_string(),
            },
            "MESSAGE_NOT_FOUND",
        ),
        (
            FilamentError::MessageAlreadyRead {
                id: "msg-1".to_string(),
            },
            "MESSAGE_ALREADY_READ",
        ),
        (
            FilamentError::AgentRunNotFound {
                id: "run-1".to_string(),
            },
            "AGENT_RUN_NOT_FOUND",
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
        (
            FilamentError::TypeMismatch {
                expected: EntityType::Task,
                actual: EntityType::Module,
                slug: Slug::new(),
            },
            "TYPE_MISMATCH",
        ),
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
    assert!(err.hint().unwrap().contains("fl list"));

    let err = FilamentError::RelationNotFound {
        id: "rel-1".to_string(),
    };
    assert!(err.hint().is_some());
    assert!(err.hint().unwrap().contains("does not exist"));

    let err = FilamentError::AgentRunNotFound {
        id: "run-1".to_string(),
    };
    assert!(err.hint().is_some());
    assert!(err.hint().unwrap().contains("does not exist"));

    let err = FilamentError::CycleDetected {
        path: "a->b".to_string(),
    };
    assert!(err.hint().is_some());

    let err = FilamentError::TypeMismatch {
        expected: EntityType::Task,
        actual: EntityType::Module,
        slug: Slug::new(),
    };
    assert!(err.hint().is_some());
    assert!(err.hint().unwrap().contains("not a task"));

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
    assert_eq!(
        FilamentError::TypeMismatch {
            expected: EntityType::Task,
            actual: EntityType::Module,
            slug: Slug::new(),
        }
        .exit_code(),
        4
    );
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
    assert_eq!(json["exit_code"], 3);
}

/// Regression: errors from the daemon protocol must preserve the original exit code,
/// error code, hint, and retryable status after round-tripping through `StructuredError`.
#[test]
fn daemon_error_preserves_exit_code_through_round_trip() {
    let test_cases: Vec<(FilamentError, i32, &str)> = vec![
        (
            FilamentError::EntityNotFound {
                id: "zzzzzzzz".to_string(),
            },
            3,
            "ENTITY_NOT_FOUND",
        ),
        (
            FilamentError::RelationNotFound {
                id: "rel-1".to_string(),
            },
            3,
            "RELATION_NOT_FOUND",
        ),
        (
            FilamentError::CycleDetected {
                path: "a->b->a".to_string(),
            },
            5,
            "CYCLE_DETECTED",
        ),
        (
            FilamentError::FileReserved {
                agent: "bot".to_string(),
                glob: "*.rs".to_string(),
            },
            6,
            "FILE_RESERVED",
        ),
        (FilamentError::ReservationExpired, 6, "RESERVATION_EXPIRED"),
        (
            FilamentError::AgentDispatchFailed {
                reason: "no binary".to_string(),
            },
            8,
            "AGENT_DISPATCH_FAILED",
        ),
        (
            FilamentError::Validation("bad input".to_string()),
            4,
            "VALIDATION_ERROR",
        ),
    ];

    for (original_err, expected_exit_code, expected_code) in &test_cases {
        // Simulate daemon: FilamentError -> StructuredError -> JSON -> StructuredError -> FilamentError
        let structured = StructuredError::from(original_err);
        let json = serde_json::to_string(&structured).unwrap();
        let deserialized: StructuredError = serde_json::from_str(&json).unwrap();
        let reconstructed = deserialized.into_error();

        assert_eq!(
            reconstructed.exit_code(),
            *expected_exit_code,
            "exit code mismatch for {expected_code}: got {}, expected {expected_exit_code}",
            reconstructed.exit_code()
        );
        assert_eq!(
            reconstructed.error_code(),
            *expected_code,
            "error code mismatch: got {}, expected {expected_code}",
            reconstructed.error_code()
        );
        assert_eq!(
            reconstructed.hint().is_some(),
            original_err.hint().is_some(),
            "hint presence mismatch for {expected_code}"
        );
        assert_eq!(
            reconstructed.is_retryable(),
            original_err.is_retryable(),
            "retryable mismatch for {expected_code}"
        );
    }
}
