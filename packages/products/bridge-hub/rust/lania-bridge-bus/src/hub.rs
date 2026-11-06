use std::collections::{BTreeSet, HashMap};

use lania_bridge_core::{bus_error, Channel, PendingTable, E_METHOD_NOT_FOUND};
use lania_bridge_protocol::{BusErrorDetail, BusFrame, Value};

use crate::channel::InProcessChannel;
use crate::endpoint::Endpoint;
use crate::policy::ProviderPolicy;

type CapabilityIndex = HashMap<String, Vec<String>>;
type TopicIndex = HashMap<String, BTreeSet<String>>;

/// Hub 运行诊断。所有计数只描述路由行为，不含语言信息。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HubDiagnostics {
    pub frames_received: u64,
    pub events_delivered: u64,
    pub requests_routed: u64,
    pub responses_routed: u64,
    pub errors_emitted: u64,
    pub bad_frames: u64,
    pub endpoints_removed: u64,
}

struct EndpointSlot {
    channel: Box<dyn Channel>,
    provides: Vec<String>,
    subscribes: Vec<String>,
    announced: bool,
}

/// 语言中立消息总线。
///
/// 维护端点表（`endpointId → Channel + provides/subscribes`）、能力表、主题表、
/// pending 表；同步 pump 驱动，不绑定 async runtime。寻址键只有 capability / topic /
/// opaque endpointId / correlation id，帧里没有任何语言字段。
pub struct BridgeHub {
    endpoints: HashMap<String, EndpointSlot>,
    /// 接入顺序。pump 按此顺序取帧，保证多端点 announce / publish / subscribe 的
    /// 相对顺序确定（HashMap 迭代顺序随机构成真实的不确定性）。
    endpoint_order: Vec<String>,
    capability_index: CapabilityIndex,
    topic_index: TopicIndex,
    pending: PendingTable,
    cursors: HashMap<String, usize>,
    policy: ProviderPolicy,
    diagnostics: HubDiagnostics,
    broadcast_to_self: bool,
}

impl Default for BridgeHub {
    fn default() -> Self {
        Self::new()
    }
}

impl BridgeHub {
    pub fn new() -> Self {
        Self {
            endpoints: HashMap::new(),
            endpoint_order: Vec::new(),
            capability_index: HashMap::new(),
            topic_index: HashMap::new(),
            pending: PendingTable::new(),
            cursors: HashMap::new(),
            policy: ProviderPolicy::default(),
            diagnostics: HubDiagnostics::default(),
            broadcast_to_self: false,
        }
    }

    pub fn with_policy(policy: ProviderPolicy) -> Self {
        let mut hub = Self::new();
        hub.policy = policy;
        hub
    }

    pub fn set_policy(&mut self, policy: ProviderPolicy) {
        self.policy = policy;
    }

    pub fn policy(&self) -> ProviderPolicy {
        self.policy
    }

    /// publish 的 event 是否也投递给发布者自身（默认 false，见设计 §3）。
    pub fn set_broadcast_to_self(&mut self, enabled: bool) {
        self.broadcast_to_self = enabled;
    }

    /// 注册一个进程内端点，返回端点句柄。id 是 opaque 字符串。
    pub fn attach(&mut self, id: impl Into<String>) -> Endpoint {
        let id = id.into();
        let (hub_side, endpoint_side) = InProcessChannel::pair();
        self.attach_channel(id.clone(), Box::new(hub_side));
        Endpoint::new(id, endpoint_side)
    }

    /// 接入任意 Channel 实现（stdio / socket / WASM …）。
    pub fn attach_channel(&mut self, id: impl Into<String>, channel: Box<dyn Channel>) {
        let id = id.into();
        if self.endpoints.contains_key(&id) {
            self.detach(&id);
        }
        self.endpoints.insert(
            id.clone(),
            EndpointSlot {
                channel,
                provides: Vec::new(),
                subscribes: Vec::new(),
                announced: false,
            },
        );
        self.endpoint_order.push(id);
    }

    pub fn detach(&mut self, id: &str) -> bool {
        let slot = match self.endpoints.remove(id) {
            Some(slot) => slot,
            None => return false,
        };
        for capability in &slot.provides {
            remove_capability(&mut self.capability_index, capability, id);
        }
        for topic in &slot.subscribes {
            remove_topic(&mut self.topic_index, topic, id);
        }
        // 动态订阅可能没同步回 slot.subscribes，兜底清理所有主题集合。
        for subscribers in self.topic_index.values_mut() {
            subscribers.remove(id);
        }
        self.topic_index.retain(|_, subscribers| !subscribers.is_empty());
        self.endpoint_order.retain(|candidate| candidate != id);
        self.pending.remove_origin(id);
        self.diagnostics.endpoints_removed += 1;
        true
    }

    pub fn endpoint_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.endpoints.keys().cloned().collect();
        ids.sort();
        ids
    }

    pub fn providers_of(&self, capability: &str) -> Vec<String> {
        self.capability_index
            .get(capability)
            .cloned()
            .unwrap_or_default()
    }

    pub fn subscribers_of(&self, topic: &str) -> Vec<String> {
        let mut result = BTreeSet::new();
        for (pattern, subscribers) in &self.topic_index {
            if topic_matches(pattern, topic) {
                result.extend(subscribers.iter().cloned());
            }
        }
        result.into_iter().collect()
    }

    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    pub fn diagnostics(&self) -> &HubDiagnostics {
        &self.diagnostics
    }

    /// 驱动一轮：从每个 Channel 非阻塞取最多一帧并路由；返回本轮是否有进展。
    pub fn pump_once(&mut self) -> bool {
        let ids = self.endpoint_order.clone();
        let mut progressed = false;
        let mut disconnected = Vec::new();

        for id in ids {
            let item = match self.endpoints.get(&id) {
                Some(slot) => slot.channel.try_recv(),
                None => continue,
            };
            match item {
                Some(Ok(frame)) => {
                    self.diagnostics.frames_received += 1;
                    self.route_frame(&id, frame);
                    progressed = true;
                }
                Some(Err(_)) => {
                    self.diagnostics.bad_frames += 1;
                    progressed = true;
                }
                None => {
                    let closed = self
                        .endpoints
                        .get(&id)
                        .map(|slot| slot.channel.is_closed())
                        .unwrap_or(false);
                    if closed {
                        disconnected.push(id);
                        progressed = true;
                    }
                }
            }
        }

        for id in disconnected {
            self.detach(&id);
        }
        progressed
    }

    /// 一直 pump 直到无帧可处理，`max_rounds` 防止意外死循环。
    pub fn pump_until_idle(&mut self, max_rounds: usize) -> usize {
        let mut rounds = 0;
        while rounds < max_rounds && self.pump_once() {
            rounds += 1;
        }
        rounds
    }

    fn route_frame(&mut self, from: &str, frame: BusFrame) {
        match frame {
            BusFrame::Announce {
                endpoint_id,
                provides,
                subscribes,
            } => {
                if endpoint_id != from {
                    self.diagnostics.bad_frames += 1;
                    return;
                }
                self.register_announce(from, provides, subscribes);
            }
            BusFrame::Subscribe { topic } => self.add_subscription(from, topic),
            BusFrame::Unsubscribe { topic } => self.remove_subscription(from, &topic),
            BusFrame::Publish { topic, payload } => self.broadcast(from, &topic, payload),
            BusFrame::Request {
                id,
                capability,
                params,
            } => self.route_request(from, id, capability, params),
            BusFrame::Response { id, result, error } => {
                self.route_response(id, result, error);
            }
            BusFrame::Bye { endpoint_id } => {
                if endpoint_id == from {
                    self.detach(from);
                } else {
                    self.diagnostics.bad_frames += 1;
                }
            }
            // event 只能由 Hub 投递给端点；端点回灌 event 视为坏帧。
            BusFrame::Event { .. } => self.diagnostics.bad_frames += 1,
        }
    }

    fn register_announce(&mut self, id: &str, provides: Vec<String>, subscribes: Vec<String>) {
        let (old_provides, old_subscribes) = match self.endpoints.get_mut(id) {
            Some(slot) => (
                std::mem::take(&mut slot.provides),
                std::mem::take(&mut slot.subscribes),
            ),
            None => return,
        };
        for capability in old_provides {
            remove_capability(&mut self.capability_index, &capability, id);
        }
        for topic in old_subscribes {
            remove_topic(&mut self.topic_index, &topic, id);
        }

        for capability in &provides {
            self.capability_index
                .entry(capability.clone())
                .or_default()
                .push(id.to_string());
        }
        for topic in &subscribes {
            self.topic_index
                .entry(topic.clone())
                .or_default()
                .insert(id.to_string());
        }
        if let Some(slot) = self.endpoints.get_mut(id) {
            slot.provides = provides;
            slot.subscribes = subscribes;
            slot.announced = true;
        }
    }

    fn add_subscription(&mut self, id: &str, topic: String) {
        if let Some(slot) = self.endpoints.get_mut(id) {
            if !slot.subscribes.contains(&topic) {
                slot.subscribes.push(topic.clone());
            }
        } else {
            return;
        }
        self.topic_index
            .entry(topic)
            .or_default()
            .insert(id.to_string());
    }

    fn remove_subscription(&mut self, id: &str, topic: &str) {
        if let Some(slot) = self.endpoints.get_mut(id) {
            slot.subscribes.retain(|candidate| candidate != topic);
        }
        remove_topic(&mut self.topic_index, topic, id);
    }

    fn broadcast(&mut self, from: &str, topic: &str, payload: Value) {
        // schema 规定 publish 的 topic 必须是具体值，通配只用于订阅。
        if topic.contains('*') {
            self.diagnostics.bad_frames += 1;
            return;
        }
        let mut targets = BTreeSet::new();
        for (pattern, subscribers) in &self.topic_index {
            if topic_matches(pattern, topic) {
                for subscriber in subscribers {
                    if self.broadcast_to_self || subscriber != from {
                        targets.insert(subscriber.clone());
                    }
                }
            }
        }
        let delivered = targets.len() as u64;
        for target in targets {
            let frame = BusFrame::Event {
                topic: topic.to_string(),
                payload: payload.clone(),
                from: from.to_string(),
            };
            self.send_to(&target, frame);
        }
        self.diagnostics.events_delivered += delivered;
    }

    fn route_request(&mut self, from: &str, id: String, capability: String, params: Value) {
        let providers: Vec<String> = self
            .capability_index
            .get(&capability)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|provider| self.endpoints.contains_key(provider))
            .collect();

        if providers.is_empty() {
            let frame = BusFrame::Response {
                id,
                result: None,
                error: Some(bus_error(
                    E_METHOD_NOT_FOUND,
                    format!("no provider for capability {capability}"),
                )),
            };
            self.diagnostics.errors_emitted += 1;
            self.send_to(from, frame);
            return;
        }

        let mut cursor = self.cursors.get(&capability).copied().unwrap_or(0);
        let chosen = self.policy.select(&providers, &mut cursor);
        self.cursors.insert(capability.clone(), cursor);
        let Some(provider) = chosen else { return };

        self.pending.insert(id.clone(), from.to_string());
        let frame = BusFrame::Request { id: id.clone(), capability, params };
        self.diagnostics.requests_routed += 1;
        if !self.send_to(&provider, frame) {
            self.pending.take(&id);
        }
    }

    fn route_response(&mut self, id: String, result: Option<Value>, error: Option<BusErrorDetail>) {
        let Some(origin) = self.pending.take(&id) else {
            self.diagnostics.bad_frames += 1;
            return;
        };
        let frame = BusFrame::Response { id, result, error };
        self.diagnostics.responses_routed += 1;
        self.send_to(&origin, frame);
    }

    fn send_to(&mut self, id: &str, frame: BusFrame) -> bool {
        let result = match self.endpoints.get(id) {
            Some(slot) => slot.channel.send(frame),
            None => return false,
        };
        match result {
            Ok(()) => true,
            Err(_) => {
                self.diagnostics.errors_emitted += 1;
                self.detach(id);
                false
            }
        }
    }
}

fn topic_matches(pattern: &str, topic: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => topic.starts_with(prefix),
        None => pattern == topic,
    }
}

fn remove_capability(index: &mut CapabilityIndex, capability: &str, id: &str) {
    if let Some(providers) = index.get_mut(capability) {
        providers.retain(|provider| provider != id);
        if providers.is_empty() {
            index.remove(capability);
        }
    }
}

fn remove_topic(index: &mut TopicIndex, topic: &str, id: &str) {
    if let Some(subscribers) = index.get_mut(topic) {
        subscribers.remove(id);
        if subscribers.is_empty() {
            index.remove(topic);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_only_matches_prefix() {
        assert!(topic_matches("event.agent_*", "event.agent_start"));
        assert!(topic_matches("event.agent_*", "event.agent_"));
        assert!(!topic_matches("event.agent_*", "event.diag"));
        assert!(topic_matches("event.diag", "event.diag"));
        assert!(!topic_matches("event.diag", "event.diag.extra"));
    }
}
