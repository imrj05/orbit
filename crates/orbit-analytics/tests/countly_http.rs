//! Integration test: the analytics client's Countly request against a local
//! mock HTTP server. No production telemetry server is contacted.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::Duration;

use orbit_analytics::{
    build_request, AnalyticsConfig, AnalyticsEvent, CountlyRequest, CountlyTransport, QueuedEvent,
    Transport,
};
use serde_json::Value;
use url::Url;

/// Accept exactly one POST, capture the raw request text, and answer 200.
fn spawn_mock_server() -> (Url, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
    let address = listener.local_addr().expect("mock address");
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut data = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            let Ok(read) = stream.read(&mut buffer) else {
                break;
            };
            if read == 0 {
                break;
            }
            data.extend_from_slice(&buffer[..read]);
            if let Some(body_start) = find_body_start(&data) {
                let headers = String::from_utf8_lossy(&data[..body_start]);
                let length = content_length(&headers);
                if data.len() >= body_start + length {
                    break;
                }
            }
        }
        let _ = tx.send(String::from_utf8_lossy(&data).into_owned());

        let body = r#"{"result":"Success"}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    });

    (Url::parse(&format!("http://{address}")).unwrap(), rx)
}

fn find_body_start(data: &[u8]) -> Option<usize> {
    data.windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|pos| pos + 4)
}

fn content_length(headers: &str) -> usize {
    headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.eq_ignore_ascii_case("content-length") {
                value.trim().parse().ok()
            } else {
                None
            }
        })
        .unwrap_or(0)
}

fn form_field(body: &str, name: &str) -> Option<String> {
    url::form_urlencoded::parse(body.as_bytes())
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
}

#[test]
fn countly_request_has_the_documented_shape() {
    let (endpoint, receiver) = spawn_mock_server();

    let mut config = AnalyticsConfig::for_endpoint(endpoint, "test-app-key");
    config.metadata = config.metadata.with_app_version("0.8.0");

    let events = vec![
        QueuedEvent::new(AnalyticsEvent::AppStarted, 1_700_000_000_000),
        QueuedEvent::new(AnalyticsEvent::AgentStarted, 1_700_000_000_100),
    ];
    let request: CountlyRequest = build_request(
        &config,
        "550e8400-e29b-41d4-a716-446655440000",
        "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
        &events,
        true,
        None,
        1_700_000_000_000,
    );

    CountlyTransport::new(&config)
        .send(&request)
        .expect("mock server accepts the request");

    let raw = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("captured request");
    assert!(
        raw.starts_with("POST /i "),
        "unexpected request line: {raw:?}"
    );

    let body_start = find_body_start(raw.as_bytes()).expect("request body");
    let body = &raw[body_start..];

    assert_eq!(form_field(body, "app_key").as_deref(), Some("test-app-key"));
    assert_eq!(
        form_field(body, "device_id").as_deref(),
        Some("550e8400-e29b-41d4-a716-446655440000")
    );
    assert_eq!(
        form_field(body, "session_id").as_deref(),
        Some("6ba7b810-9dad-11d1-80b4-00c04fd430c8")
    );
    assert_eq!(form_field(body, "begin_session").as_deref(), Some("1"));

    let metrics: Value =
        serde_json::from_str(&form_field(body, "metrics").expect("metrics")).unwrap();
    assert_eq!(metrics["_app_version"], "0.8.0");
    assert_eq!(metrics["_device_type"], "desktop");

    let events: Value = serde_json::from_str(&form_field(body, "events").expect("events")).unwrap();
    let events = events.as_array().expect("events array");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["key"], "app_started");
    assert_eq!(events[1]["key"], "agent_started");
    let segmentation = &events[0]["segmentation"];
    assert_eq!(segmentation["app_version"], "0.8.0");
    assert_eq!(segmentation["os"], std::env::consts::OS);
    assert_eq!(segmentation["architecture"], std::env::consts::ARCH);
}

#[test]
fn prohibited_data_is_absent_from_the_payload() {
    let (endpoint, receiver) = spawn_mock_server();
    let config = AnalyticsConfig::for_endpoint(endpoint, "test-app-key");
    let events = vec![
        QueuedEvent::new(
            AnalyticsEvent::AgentFailed {
                reason: orbit_analytics::AgentFailure::Provider,
            },
            1,
        ),
        QueuedEvent::new(
            AnalyticsEvent::SettingsChanged {
                setting: orbit_analytics::SettingId::Appearance,
            },
            2,
        ),
    ];
    let request = build_request(
        &config,
        "install-id",
        "session-id",
        &events,
        false,
        Some(12),
        3,
    );
    CountlyTransport::new(&config).send(&request).unwrap();
    let raw = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
    let body_start = find_body_start(raw.as_bytes()).unwrap();
    let body = &raw[body_start..];

    // No paths, emails, URLs, or secret-looking values anywhere in the payload.
    for marker in ["/Users/", "/home/", "@", "://", "ssh-", "Bearer ", "ghp_"] {
        assert!(
            !body.contains(marker),
            "payload unexpectedly contains {marker:?}: {body}"
        );
    }

    // Only the whitelisted segmentation keys appear.
    let events: Value = serde_json::from_str(&form_field(body, "events").unwrap()).unwrap();
    for event in events.as_array().unwrap() {
        let segmentation = event["segmentation"].as_object().unwrap();
        for key in segmentation.keys() {
            assert!(
                matches!(
                    key.as_str(),
                    "app_version"
                        | "os"
                        | "architecture"
                        | "environment"
                        | "locale"
                        | "reason"
                        | "setting"
                ),
                "unexpected segmentation key {key:?}"
            );
        }
    }
}
