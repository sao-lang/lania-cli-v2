//! 配置文件发现规则。
//!
//! 这个模块很小，但语义很重要：
//! - 它定义了宿主默认会去哪些文件名里查找 `lan.config.*`
//! - 这些候选项会被配置加载、bridge 请求和文档输出共同复用

pub(crate) const LAN_CONFIG_SEARCH_PLACES: [&str; 4] = [
    "lan.config.js",
    "lan.config.cjs",
    "lan.config.json",
    "lan.config.ts",
];

pub(crate) fn lan_config_search_places() -> Vec<String> {
    // 这里返回 owned Vec，便于调用方在 hook/waterfall 中增删候选项，
    // 避免直接暴露静态数组导致后续扩展受限。
    LAN_CONFIG_SEARCH_PLACES
        .iter()
        .map(|place| (*place).to_string())
        .collect::<Vec<String>>()
}
