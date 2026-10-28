# 只读用量统计与订阅额度

## Status

数据源与费用显示被 [本机原生用量与设置页权益](2026-10-26-native-usage-and-settings.md) 替代；以下验证仅适用于原有版本。

Reset redemption and calibration metadata extend the original read-only boundary: [Codex entitlements and calibration](2026-10-23-codex-entitlements-and-calibration.md).

Refresh ownership superseded by [workspace status refresh](2026-10-23-workspace-status-refresh.md).

Implemented; Windows debug build and targeted runtime checks passed.

## Context

用户需要在轻量终端里查看常用工具的用量及 Codex 剩余额度，而无需再切换到
CC Switch 和 codex-status。数据来自已有工具，不新增代理、模型预测、账户管理
或后台常驻采集服务。

## Evidence

- 本机 CC Switch 3.20.4 的 `usage_stats.rs`、`sql_helpers.rs` 和数据库字段确认：
  用量来自代理与本地会话两类记录；存在跨来源去重、缓存输入口径及历史日汇总。
- 本地 codex-status 的 `src/main/services/quota.ts` 使用 ChatGPT usage 响应中的
  `rate_limit` 窗口，并在异步响应返回后重新检查账号和凭据。
- [`cc_switch.sql`](../../../../nebula_app/src/ai_usage/cc_switch.sql) 保留统计口径和
  CC Switch 上游 MIT 声明；查询只选择用量字段。
- [`codex.rs`](../../../../nebula_app/src/ai_usage/codex.rs) 负责额度请求及响应验证。

## Decision

[`ai_usage`](../../../../nebula_app/src/ai_usage.rs) 拥有外部数据适配，GPUI 工作区
只负责显示、日期范围与异步任务生命周期。复用现有 ureq、rusqlite、chrono、
base64、zeroize，以及 Codex 配置目录和系统/环境代理解析，不添加依赖。

CC Switch 以只读 SQLite 连接访问默认目录的 `cc-switch.db`，不迁移、导入、
回填、清理或写入数据库。一个 SELECT 同时读取明细和已归档的完整历史日，
保留十分钟跨来源去重及 `input_token_semantics` 的缓存归一规则。
Claude Desktop 的记录按 CC Switch 的展示口径合入 Claude Code。
界面只列 Claude、Codex、Gemini 和 OpenCode；支持今日与本月，费用明确为 USD
估算值，不作为订阅余额或实际账单。

Codex 从已有 `codex_home()` 定位 `auth.json`。仅识别 ChatGPT 登录；API Key
模式不伪装成可查询的订阅额度。JWT 只用于账号一致性，不代替服务端鉴权。
HTTP 使用 HTTPS、固定目标、零重定向、20 秒总时限和 1 MiB JSON 上限；不刷新
或写入凭据，不显示、记录或把凭据传给视图。响应下载完成后重新读取凭据，
拒绝换号或续期前发出的响应，检查响应账号及百分比范围。

额度与本地统计各有一个工作区持有的后台任务，独立显示结果。切换日期范围
只刷新统计，不重新请求额度。打开与手动刷新才读取数据；关闭浮层、焦点离开
或工作区销毁会取消 UI 等待，旧结果不能覆盖新状态。已经执行的阻塞 HTTP
不会被强制中断，但有总时限，且被取消的宿主任务不再提交其结果。

浮层使用现有原生样式与滚动容器。展开侧栏时入口在底部；收起侧栏或采用顶部
标签时，入口位于标题栏，避免同时出现两个入口。数据为全局工具/账号统计，
不会因项目筛选被误标为该项目花费。

## Rejected alternatives

- 对数据库 Token 字段直接求和：缓存会重复计入，代理和会话日志也可能重复。
- 只读取日汇总：正常情况下当日记录仍在明细表，无法得到今日用量。
- 解析并重新计价所有 CLI 会话文件：复制 CC Switch 的采集与定价权威。
- 单个任务串行读取所有数据：联网额度慢时会挡住本地统计。
- 失败时显示 0% 或跨账号沿用缓存：会把未知额度误报成真实额度。

## Consequences

统计只覆盖 CC Switch 已收集的记录；Pebrel 不替 CC Switch 触发会话导入。
目前只支持默认数据库路径与已核对的 3.20 数据结构；不兼容时明确报错。
订阅额度目前仅支持 Codex 的文件式 ChatGPT 登录；其他工具提供使用统计。
ChatGPT usage 不是本项目维护的稳定协议，字段变化时显示不可用，不猜测额度。

## Validation

Windows debug 产品构建通过，相关自动测试共 275 项通过。适配器测试覆盖跨来源
去重、历史日汇总、缓存口径、额度窗口和异常数据；GPUI 测试覆盖成功/失败浮层的
布局、内部点击、外部点击关闭及 Escape，3 项工作区测试保留原有忽略标记。

以生产 SQL 对本机 CC Switch 数据库执行只读的今日/本月查询成功；显式运行真实
Codex 额度请求测试，取得一个有效窗口。凭据由适配器读取，未输出或修改凭据。
隔离配置下原生程序启动与 Runtime API 命令执行正常。原生窗口自动化管道不可用，
额度的原生视觉展示与手机实机体验仍需人工验收。

## Supersedes

None.

## Revisit when

CC Switch 更改数据结构或统计口径，需要自定义数据库路径、系统钥匙串登录、
更多订阅工具额度，或跨窗口共享请求缓存时。
