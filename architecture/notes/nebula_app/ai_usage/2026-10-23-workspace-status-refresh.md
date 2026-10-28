# 工作区持续用量与会话请求指标

## Status

数据源、刷新间隔及性能口径被 [本机原生用量与设置页权益](2026-10-26-native-usage-and-settings.md) 替代。

Implemented in source; native build and runtime acceptance pending for this change.

## Context

用户要求左下角持续显示已使用额度，并在终端对话区域显示 tok/s、首 token 耗时。
此前数据只在打开浮层时读取，关闭浮层即释放，无法驱动持续状态区。

## Evidence

- 本机 CC Switch 字段核对存在 `session_id`、`output_tokens`、`latency_ms`、
  `first_token_ms`、`duration_ms` 和来源标记；Codex 会话记录使用原生 UUID。
- [CC Switch 3.20.4 计时逻辑](https://github.com/farion1231/cc-switch/blob/v3.20.4/src/components/usage/format.ts)
  优先使用生成耗时，其次使用总耗时扣除首 token 等待，缺少首 token 时只能估算。
- 终端已有 `session_agent()`，无需根据屏幕字符数量推断 tokens 或会话身份。

## Decision

数据快照、轮询和请求任务由工作区拥有。原生开窗入口启动轮询，工作区销毁时
释放任务；关闭浮层仅释放浮层焦点。托盘隐藏期间跳过读取，单个来源不重叠请求。
指标每 5 秒、用量每 15 秒、Codex 额度每 60 秒读取；429 后额度间隔为 300 秒。
间隔常量同时驱动界面说明，未新增后台服务或依赖。

额度读取开始即移除旧值。后台检查 auth.json 的修改时间与长度，只用于缓存失效；
文件变化会取消旧等待并重新读取。身份与凭据校验仍由 Codex 请求适配器负责。
视图不持有凭据，失败、失效或重置等待不会显示成 0% 已用。

本地统计和请求指标继续只读 CC Switch。`metrics.rs` 只查询当前已知原生会话
身份对应的最近一次成功请求，不借用同工具其他会话的数据。输出速度优先取生成
耗时，再取总耗时减首 token；会话日志、非流式及缺少首 token 的估算带 `≈`。
缺少字段显示 `—`。该界面是最近完成请求的快照，不是逐 token 流式遥测。
SSH 会话不关联主机的本地记录。

左下角显示 Codex 窗口已用比例和所选期间全部工具估算费用，点击打开详细浮层。
顶部标签或收起侧栏时使用底部紧凑状态区，避免重复入口。

## Rejected alternatives

- 从终端字符或回合总耗时估算 token 速率：不是实际模型输出计数。
- 没有同会话记录时展示同工具最近请求：会污染会话归属。
- 将 API 成本当作订阅剩余额度，或缺失字段显示零：误报未知状态。
- 为持续状态另起采集进程：当前可复用已有数据库与工作区生命周期。

## Consequences

实际新鲜度受 CC Switch 采集间隔约束。订阅余额仍只支持 Codex 文件式 ChatGPT
登录，其他工具提供使用统计。每个工作区独立轮询，跨窗口请求尚未合并。

## Validation

核对了真实数据库的字段、来源与原生 ID 形态，仅输出聚合信息，未读取或输出凭据。
源码经 rustfmt 解析，架构静态检查与目录 JSON 解析通过。未在本轮重新构建、运行
产品或执行测试，不据此声称额度切换与原生视觉已验收。

## Supersedes

[只读用量适配](2026-10-23-read-only-usage-adapters.md) 中按需读取、浮层拥有任务的
生命周期决定；统计归一、来源去重、凭据与 HTTP 边界仍由原适配器实现。

## Revisit when

需要逐 token 遥测、更多订阅额度、独立采集或跨窗口合并请求时。
