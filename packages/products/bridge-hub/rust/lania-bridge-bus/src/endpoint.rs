use lania_bridge_core::Channel;
use lania_bridge_protocol::{BusErrorDetail, BusFrame, Value};

use crate::channel::InProcessChannel;
use crate::runtime::EndpointRuntime;

/// 语言侧业务只需实现这两个回调；帧编解码、id、错误包装由生成骨架负责。
pub trait Handler {
    fn on_request(&mut self, capability: &str, params: Value) -> Result<Value, BusErrorDetail>;
    fn on_event(&mut self, topic: &str, payload: Value, from: &str);
}

/// 进程内端点句柄。全双工：既能提供能力，也能发请求、发事件、订阅主题。
pub struct Endpoint {
    id: String,
    channel: InProcessChannel,
    runtime: EndpointRuntime<Box<dyn FnMut(BusFrame) + Send>>,
}

impl Endpoint {
    pub fn new(id: String, channel: InProcessChannel) -> Self {
        let outgoing = channel.clone();
        let sink: Box<dyn FnMut(BusFrame) + Send> = Box::new(move |frame| {
            let _ = outgoing.send(frame);
        });
        Self {
            runtime: EndpointRuntime::new(id.clone(), sink),
            id,
            channel,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn announce(&mut self, provides: Vec<String>, subscribes: Vec<String>) {
        self.runtime.announce(provides, subscribes);
    }

    pub fn announce_str(&mut self, provides: &[&str], subscribes: &[&str]) {
        self.announce(
            provides.iter().map(|s| (*s).to_string()).collect(),
            subscribes.iter().map(|s| (*s).to_string()).collect(),
        );
    }

    pub fn subscribe(&mut self, topic: &str) {
        self.runtime.subscribe(topic);
    }

    pub fn unsubscribe(&mut self, topic: &str) {
        self.runtime.unsubscribe(topic);
    }

    pub fn publish(&mut self, topic: &str, payload: Value) {
        self.runtime.publish(topic, payload);
    }

    /// 返回 correlation id；响应帧由 Hub 按 pending 表送回本端点。
    pub fn request(&mut self, capability: &str, params: Value) -> String {
        self.runtime.request(capability, params)
    }

    pub fn respond(&mut self, id: String, outcome: Result<Value, BusErrorDetail>) {
        self.runtime.respond(id, outcome);
    }

    pub fn heartbeat(&mut self) {
        self.runtime.heartbeat();
    }

    pub fn bye(&mut self) {
        self.runtime.bye();
    }

    pub fn try_recv(&self) -> Option<BusFrame> {
        match self.channel.try_recv() {
            Some(Ok(frame)) => Some(frame),
            _ => None,
        }
    }

    /// 处理一帧：request 交给业务回调并自动回 response，event 交给业务回调。
    pub fn serve_once(&mut self, handler: &mut dyn Handler) -> bool {
        let frame = match self.try_recv() {
            Some(frame) => frame,
            None => return false,
        };
        match frame {
            BusFrame::Request {
                id,
                capability,
                params,
            } => {
                let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    handler.on_request(&capability, params)
                }));
                let outcome = match caught {
                    Ok(Ok(value)) => Ok(value),
                    Ok(Err(err)) => Err(err),
                    Err(_) => Err(self.runtime.wrap_internal("handler panicked")),
                };
                self.runtime.respond(id, outcome);
            }
            BusFrame::Event { topic, payload, from } => {
                handler.on_event(&topic, payload, &from);
            }
            _ => {}
        }
        true
    }

    pub fn serve_until_idle(&mut self, handler: &mut dyn Handler, max_frames: usize) -> usize {
        let mut served = 0;
        while served < max_frames && self.serve_once(handler) {
            served += 1;
        }
        served
    }
}
