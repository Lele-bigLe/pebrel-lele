# 本机原生用量与设置页权益边界

## Status

Implemented in source. 本轮尚未构建或进行真实权益、兑换及视觉验收。

## Context

用户要求用量使用 Pebrel 自身读取的原生记录，容量与重置卡移至设置；配置仍由
CC Switch 管理。原先读取 CC Switch 数据库的决定已被这一明确要求替代。

## Evidence

- [`native.rs`](../../../../nebula_app/src/ai_usage/native.rs) 与子模块聚合本机记录。
- [`workspace/usage.rs`](../../../../nebula_app/src/gpui_shell/workspace/usage.rs) 持有轮询。
- [`codex_usage.rs`](../../../../nebula_app/src/gpui_shell/settings_pane/codex_usage.rs) 持有设置页权益状态。
- 已核对本机 Codex npm 平台包存在嵌套布局和 `vendor/<triple>/bin/codex.exe`。

## Decision

原生文件及 OpenCode 数据库只读；不复制正文、不新增采集进程或统计数据库。
聚合缓存按进程和本地月份持有，增量读取有单轮/单文件预算。文件截断、替换和
已读边界变动使旧累计失效；最后 64 字节锚点用于识别增长过程中改写的已读尾部。
Gemini 整体 JSON 更新成功后替换快照，解析失败时不继续展示旧快照为已知下界。

Codex 累计 input/output 做差分，缓存和 reasoning 子集不重复相加；fork 回放尚不
归入完整统计，明确标记 partial。Claude 按消息 ID 增量去重，Gemini 使用原生 total，
OpenCode 按非重叠原生桶汇总。预算、缺失或不可读记录用 partial/下界语义表达。
共享历史扫描向用量调用方暴露目录/文件元数据读取失败；不存在的原生存储与
无法读取分开处理，OpenCode 也不把权限错误当成数据库不存在。
这不是官方账单；没有价格契约时不显示美元估算。

本机统计和最近完成回合性能共用工作区的 5 秒任务；官方窗口额度独立 60 秒读取，
429 退避 300 秒。TTFT 只用原生字段，tok/s 标记为完整回合耗时上的约算。
容量与重置卡操作由设置实体拥有，用 generation 和账号快照拒绝过期结果；不自动
兑换。原有幂等记录、账号校验和官方额度 HTTP 边界保持权威。

## Rejected alternatives

- 继续复用 CC Switch 数据库或重复维护供应商/路由配置：违背最新职责决定。
- 从终端字符估算 token、将 API 价格换算订阅余额、缺失额度显示零：会误导。
- 将 Git/历史或当前项目筛选套用到全机用量摘要：混淆统计范围。

## Consequences

本机记录的完整性和格式决定统计范围；缺失信息不补造。设置页关闭/销毁与账号
变化不会自动重试兑换。目录遍历和文件预算仍不等于覆盖其他设备的账户使用量。

## Validation

核对源码、调用边界及原生格式；本轮未运行构建、测试、App Server 或真实兑换。

## Supersedes

- [只读用量适配](2026-10-23-read-only-usage-adapters.md) 的 CC Switch 数据源和费用显示。
- [工作区指标刷新](2026-10-23-workspace-status-refresh.md) 的统计来源、间隔和性能口径。
- 不替代 [权益与校准](2026-10-23-codex-entitlements-and-calibration.md) 的幂等/采样规则。

## Revisit when

原生格式变更、需要完整 fork 计量或跨窗口共享请求时。
