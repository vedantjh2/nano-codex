use super::*;
use tokio::net::TcpStream;

#[tokio::test]
async fn callback_ignores_unrelated_requests_and_wrong_state() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let pending = tokio::spawn(async move {
        tokio::time::timeout(
            Duration::from_secs(2),
            callback(&listener, "expected-state"),
        )
        .await
        .unwrap()
        .unwrap()
    });
    for (path, expected_status) in [
        ("/favicon.ico", "400"),
        ("/auth/callback?state=attacker&code=wrong", "400"),
        (
            "/auth/callback?state=expected-state&code=good&client_id=issued",
            "200",
        ),
    ] {
        let mut connection = TcpStream::connect(address).await.unwrap();
        connection
            .write_all(format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").as_bytes())
            .await
            .unwrap();
        let mut response = String::new();
        connection.read_to_string(&mut response).await.unwrap();
        assert!(response.starts_with(&format!("HTTP/1.1 {expected_status}")));
    }
    let result = pending.await.unwrap();
    assert_eq!(result["code"], "good");
    assert_eq!(result["client_id"], "issued");
}
