//! 动态 hook 逻辑的薄入口。
//!
//! - `hook_invokers` 负责 bridge / inline invoker 的执行实现
//! - `hook_registration` 负责把 target 中的 hook 声明注册到 HookBus
//!
//! 拆分之后，这个文件只扮演目录边界与 re-export 角色，
//! 避免原来的 `hooks.rs` 同时承担“定义 invoker”“调用 bridge”“解析 target hook 声明”
//! 三种职责。

#[path = "hook_invokers.rs"]
mod hook_invokers;
#[path = "hook_registration.rs"]
mod hook_registration;

pub(in crate::runtime) use self::hook_registration::register_dynamic_target_hooks;
