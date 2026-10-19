use nano_codex::setup::local_url;

#[test]
fn local_endpoints_accept_a_port_host_or_explicit_api_root() {
    assert_eq!(local_url("", 8000).unwrap(), "http://127.0.0.1:8000/v1");
    assert_eq!(
        local_url("30000", 8000).unwrap(),
        "http://127.0.0.1:30000/v1"
    );
    assert_eq!(
        local_url("cluster.local:9000", 8000).unwrap(),
        "http://cluster.local:9000/v1"
    );
    assert_eq!(
        local_url("https://cluster.example", 8000).unwrap(),
        "https://cluster.example/v1"
    );
    assert_eq!(
        local_url("https://cluster.example/api/v1/", 8000).unwrap(),
        "https://cluster.example/api/v1"
    );
}

#[test]
fn invalid_ports_and_embedded_credentials_are_rejected() {
    for input in [
        "0",
        "65536",
        "file:///tmp/model",
        "https://key:secret@cluster.example/v1",
        "https://cluster.example/v1?token=secret",
    ] {
        assert!(local_url(input, 8000).is_err(), "Accepted {input}");
    }
}
