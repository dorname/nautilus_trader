//! UT-BUS-01：消息总线三消息模式（规格 logos/resources/test/unit/unit-test-cases.md）。
//!
//! pub/sub（subscribe_any + publish_any 按模式匹配）、p2p（register_any + send_any
//! endpoint 直连）、req/resp（correlation_id 注册/取回响应处理器）各自收发正确，
//! 且不同通道互不串扰。msgbus 为 thread-local 单例，单线程测试内使用。

use nautilus_common::msgbus::{self, stubs::get_call_check_handler};
use nautilus_core::UUID4;
use nautilus_research_testkit::case;

#[test]
fn ut_bus_01_three_messaging_patterns() {
    case("UT-BUS-01", || {
        // ① pub/sub：模式订阅 + 按主题发布
        let (pubsub_handler, pubsub_check) = get_call_check_handler(None);
        msgbus::subscribe_any("data.*".into(), pubsub_handler, None);
        msgbus::publish_any("data.quotes".into(), &"pubsub-message");
        assert!(pubsub_check.was_called(), "pub/sub 订阅者必须收到匹配主题的发布");

        // ② p2p：endpoint 直连注册 + 定向发送
        let (p2p_handler, p2p_check) = get_call_check_handler(None);
        msgbus::register_any("BaselineP2PEndpoint".into(), p2p_handler);
        assert!(msgbus::has_endpoint("BaselineP2PEndpoint"), "endpoint 注册可查");
        msgbus::send_any("BaselineP2PEndpoint".into(), &"p2p-message");
        assert!(p2p_check.was_called(), "p2p 定向接收者必须收到");

        // ③ req/resp：按 correlation_id 注册响应处理器，可取回并调用
        let (resp_handler, resp_check) = get_call_check_handler(None);
        let correlation_id = UUID4::new();
        msgbus::register_response_handler(&correlation_id, resp_handler);
        let fetched = msgbus::get_message_bus()
            .borrow()
            .get_response_handler(&correlation_id)
            .expect("按 correlation_id 取回响应处理器")
            .clone();
        fetched.handle(&"resp-message" as &dyn std::any::Any);
        assert!(resp_check.was_called(), "req/resp 处理器按 correlation_id 命中");

        // 无串扰：channel A 的订阅者不收 channel B 的消息
        let (a_handler, a_check) = get_call_check_handler(None);
        msgbus::subscribe_any("chan.a".into(), a_handler, None);
        msgbus::publish_any("chan.b".into(), &"noise");
        assert!(!a_check.was_called(), "channel A 不得收到 channel B 的消息");
        msgbus::publish_any("chan.a".into(), &"signal");
        assert!(a_check.was_called(), "channel A 收到自己的消息");
    });
}
