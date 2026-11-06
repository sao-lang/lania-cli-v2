//! Bridge Hub 帧协议的薄壳 crate。
//!
//! 帧类型、编解码、错误码全部来自 `protocol/gen/rust/protocol.rs`（由
//! `protocol/gen.mjs` 从 `protocol/schema.json` 生成），本文件只做 `include!`。
//! 生成物禁止手改；改协议请改 schema 后重新运行生成器。

include!("../../../protocol/gen/rust/protocol.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_roundtrip_without_language_fields() {
        let cases = vec![
            BusFrame::Announce {
                endpoint_id: "cli".into(),
                provides: vec!["tool.exec".into()],
                subscribes: vec!["event.agent_*".into()],
            },
            BusFrame::Subscribe { topic: "event.diag".into() },
            BusFrame::Unsubscribe { topic: "event.diag".into() },
            BusFrame::Publish {
                topic: "event.diag".into(),
                payload: serde_json::json!({ "text": "hello" }),
            },
            BusFrame::Event {
                topic: "event.diag".into(),
                payload: serde_json::json!({ "text": "hello" }),
                from: "cli".into(),
            },
            BusFrame::Request {
                id: "cli-1".into(),
                capability: "agent.prompt".into(),
                params: serde_json::json!({ "text": "hi" }),
            },
            BusFrame::Response {
                id: "cli-1".into(),
                result: Some(serde_json::json!({ "ok": true })),
                error: None,
            },
            BusFrame::Response {
                id: "cli-2".into(),
                result: None,
                error: Some(bus_error(E_METHOD_NOT_FOUND, "no provider")),
            },
            BusFrame::Bye { endpoint_id: "cli".into() },
        ];

        for frame in cases {
            let encoded = encode_frame(&frame).expect("encode");
            let value: serde_json::Value = serde_json::from_str(&encoded).expect("valid json");
            let object = value.as_object().expect("frame is an object");
            assert_eq!(object["kind"], frame.kind());
            for forbidden in ["language", "lang", "runtime", "runtimeKind", "protocolVersion"] {
                assert!(!object.contains_key(forbidden), "frame {encoded} contains {forbidden}");
            }
            assert_eq!(decode_frame(&encoded).expect("decode"), frame);
        }
    }

    #[test]
    fn descriptor_covers_all_frames() {
        assert_eq!(FRAME_KINDS.len(), 8);
        assert!(FRAME_KINDS.contains(&"announce"));
        assert!(FRAME_KINDS.contains(&"response"));
    }
}
