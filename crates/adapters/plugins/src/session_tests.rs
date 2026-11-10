use std::time::Duration;

use serde_json::json;

use super::messages;
use crate::framing;

#[tokio::test]
async fn a_plugin_that_writes_faster_than_it_is_read_is_given_up_on() {
    let (mut plugin, host) = tokio::io::duplex(1 << 16);
    for n in 0..10 {
        framing::write(&mut plugin, &json!({"jsonrpc": "2.0", "method": "log", "params": {"n": n}})).await.unwrap();
    }
    let mut incoming = messages(host, 4);
    tokio::time::sleep(Duration::from_millis(50)).await;
    let mut read = Vec::new();
    while let Some(message) = incoming.recv().await {
        read.push(message);
    }
    assert_eq!(read.len(), 4, "{read:?}");
    assert!(read[..3].iter().all(Result::is_ok), "{read:?}");
    assert_eq!(read[3], Err("it wrote more than 3 messages the host had not read yet".to_owned()));
}

#[tokio::test]
async fn messages_read_in_time_all_arrive_in_order() {
    let (mut plugin, host) = tokio::io::duplex(1 << 16);
    let mut incoming = messages(host, 4);
    for n in 0..10 {
        framing::write(&mut plugin, &json!({"n": n})).await.unwrap();
        assert_eq!(incoming.recv().await, Some(Ok(json!({"n": n}))));
    }
}
