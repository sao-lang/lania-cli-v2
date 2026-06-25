//! `parser` 模块的最小测试入口。
//!
//! 这个文件的存在主要是为了匹配 `parser.rs` 里的 `#[cfg(test)] mod tests;`：
//! - 没有它时，`cargo test`/rust-analyzer 会在 test 配置下报 “module not found”。
//! - 后续如果要补更完整的测试，可以继续往这里加用例。
//!
//! 说明：这里先放一个非常轻量的 sanity check，避免未来再次出现“测试模块为空但没人注意”的情况。
//! 它不验证复杂行为，只确保基本的命令树构建能在最小输入下跑通。

use super::*;

#[test]
fn build_cli_smoke() {
    let cli = build_cli("lan", "about", "0.0.0", &[], "en");
    // 最小 smoke：确保命令对象能构造出来，并且基本字段可读取。
    assert_eq!(cli.get_name(), "lan");
}
