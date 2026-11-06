//! 测试专用 crate：命令包一致性 conformance 检查。
//!
//! 当前承接两项职责（原 `lania-command` 的 dev-dependencies 测试，见
//! `docs/design/package-split-command-atomization.zh-CN.md` §5.1 A.5(7) 步 5）：
//!
//! 1. 本地化覆盖测试：组装真实内建命令树（与 `lania-cli` 同一注册顺序），
//!    断言 zh 本地化覆盖每一条 about / option help / arg help / example 文案。
//! 2. 作为后续命令子包逐个拆包时的「命令树一致性」检查落点：
//!    新命令/新选项/新示例加入任何命令包时，无需改动本 crate 即自动被覆盖。
//!
//! 依赖方向说明：本 crate 是测试汇合点，允许反向引用全部命令插件与
//! `lania-host`——这正是把这类测试从 `lania-command` 移出的原因：
//! `lania-command` 必须保持干净 leaf，命令子包才能无环依赖它。
//!
//! 本 crate 不产生任何发布产物，不作为任何业务 crate 的依赖。
