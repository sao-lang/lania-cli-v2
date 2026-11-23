# .conductor/ · conductor 产物目录（分类固定，禁止在其他位置落文件）

| 子目录/文件                                      | 内容                                                                                         | 版本库 |
| ------------------------------------------------ | -------------------------------------------------------------------------------------------- | ------ |
| `session.json`                                   | 会话指针 `{"sid":"8位短hash"}`                                                               | 忽略   |
| `overviews/YYYY-MM-DD/<hash>-<slug>.md`          | 会话主文件（**一个 sid 只允许一个主文件**）                                                  | 跟踪   |
| `overviews/.../<任意>.<后缀>.md`                 | 附属文件，后缀只限：`.details/.review/.review2/.review3/.reviewN/.questions/.debts/.changes` | 跟踪   |
| `contracts/YYYY-MM-DD/<hash>-<slug>.contract.md` | 六段变更契约（同会话 merge 同一文件）                                                        | 跟踪   |
| `ledgers/commitment-ledger.json`                 | 承诺台账（防漏引擎）                                                                         | 忽略   |
| `ledgers/migration-ledger.json`                  | 迁移台账（场景 7）                                                                           | 忽略   |
| `baselines/purity-baseline*.json`                | 源码纯净存量基线                                                                             | 忽略   |
| `archive/`                                       | 历史归档（旧 `.overview-archive/` 迁入）                                                     | 忽略   |

**纪律**：① 同一 sid 只允许一个主文件，新主题追加为新章节，要分开就 `--new-session`；
② 附属内容（详情/独立 review/问题清单）一律用上表注册后缀命名，或内嵌进主文件章节，禁止再造无后缀新文件；
③ 子代理产出回写主 overview（独立 review 报告内嵌），不单独落盘；
④ 所有路径以 `overview_path.py` 返回值为准，禁止手拼。`--audit` 可随时自查违规。
