/// 端点生命周期记录。状态只描述「是否声明过能力」，不描述语言或运行时。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EndpointStatus {
    #[default]
    Attached,
    Announced,
    Gone,
}

#[derive(Debug, Clone, Default)]
pub struct EndpointRecord {
    pub id: String,
    pub provides: Vec<String>,
    pub subscribes: Vec<String>,
    pub status: EndpointStatus,
}

impl EndpointRecord {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ..Self::default()
        }
    }
}
