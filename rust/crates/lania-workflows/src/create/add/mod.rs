//! `add` 生成器重构（§4.5）：框架探测 → 生成器解析 → 命名推导 → 多文件产物 → 注册副作用。
//!
//! 这一层只放**纯逻辑**（表驱动 + 字符串渲染），不碰 bridge、不碰写盘：
//! - `generators`：仓库内唯一一处生成器定义
//! - `templates`：每个产物的代码骨架
//! - `names`：命名推导（Nest 风格）
//! - `detect`：框架探测与回落
//! - `render`：渲染与锚点注册

pub(crate) mod detect;
pub(crate) mod generators;
pub(crate) mod names;
pub(crate) mod render;
pub(crate) mod templates;

pub(crate) use detect::{detect_framework, FrameworkDetection};
pub(crate) use generators::{
    all_generators, find_generator, Generator, GeneratorQuestionKind, LANIA_IMPORTS_MARKER,
};
pub(crate) use names::{name_variants, NameVariants};
pub(crate) use render::{
    insert_after_marker, render_generator, render_registrations, ProjectRenderContext,
    RenderContext,
};
