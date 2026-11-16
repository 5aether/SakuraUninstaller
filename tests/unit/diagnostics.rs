use super::{payload, valid_destination};

#[test]
fn validates_destination_and_limits_payload_to_aggregate_counts() {
    assert!(valid_destination("telemetry.example.com", "/v1/counts"));
    for (host, path) in [
        ("https://telemetry.example.com", "/v1/counts"),
        ("user@telemetry.example.com", "/v1/counts"),
        ("telemetry.example.com", "//other.example.com"),
        ("telemetry.example.com", "/v1/../counts"),
        ("telemetry.example.com", "/v1/counts?token=test"),
    ] {
        assert!(!valid_destination(host, path));
    }
    assert_eq!(
        payload([3, 1]),
        serde_json::json!({"schema": 1, "completed": 3, "errors": 1})
    );
}
