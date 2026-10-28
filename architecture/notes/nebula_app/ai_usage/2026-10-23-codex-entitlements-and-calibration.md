# Codex 权益操作与本机容量校准

## Status

Windows debug product build passed; real endpoint and native acceptance pending.

## Context

用户将供应商、路由、MCP、Skills 和计费配置继续交给 CC Switch，要求 Pebrel 补充 Codex 重置卡和模型容量估算。
订阅的百分比窗口无法通过 API token 单价换算为官方固定 token 余额。

## Evidence

- [官方 App Server 协议](https://learn.chatgpt.com/docs/app-server) 定义账号、动态额度窗口、重置卡及带幂等键的兑换接口。
- [官方定价说明](https://learn.chatgpt.com/docs/pricing) 将订阅消耗和 API 定价分开，并说明模型、推理、上下文和缓存会影响消耗。
- 本地 codex-status 的 `services/codex.ts` 和 `services/automation.ts` 已采用先落盘请求标识、再兑换、未知结果复用原请求的流程。
- [Usage Tools for Codex](https://github.com/modeismodei/usage-tools-for-codex) 的分析说明采用连续快照和 token/百分比增量校准，并强调采样中断与外部活动的影响。

## Decision

[`codex_entitlements`](../../../../nebula_app/src/ai_usage/codex_entitlements.rs) 使用短生命周期 Codex 原生 App Server。
只连接已发现的原生可执行文件，不把路径拼成 shell 命令；复用 Codex 文件登录和配置目录。
初始化后查询账号与权益，不创建线程、不发模型请求，关闭工具、Hooks、Apps 和多智能体功能。
所有 RPC 有总时限、消息大小和数量上限，子进程离开作用域会被回收。
普通额度轮询继续使用既有 HTTPS 适配器；卡片只在打开面板、刷新或兑换结果后查询。

卡片数量采用官方 availableCount，明细为空或缺失保留区别，明细不全时禁止新兑换。
展示可用卡的有效期并优先推荐最早到期卡，用户选择后在原生确认框再次确认账号与卡片。
兑换事务使用进程生存期文件锁，在请求前原子保存账号摘要、卡片 ID 和 UUID 幂等键。
超时、换号和未知结果保留请求；重试不能换卡或生成新键。
只有官方明确返回已知 outcome 才清除待确认记录，然后重新读取额度与权益。
读取和兑换都检查完整凭据与响应账号；视图不接收 token，不改写 CC Switch 或 Codex 配置。

[`capacity`](../../../../nebula_app/src/ai_usage/capacity.rs) 仅校准当前账号、套餐、窗口、模型、推理强度和速度档位。
读取本机 rollout 的有界尾部，按累计 token 增量计数，缓存属于输入、推理属于输出，不重复相加。
无法确认来源、回放/子智能体、模型混用、缺失元数据、截断覆盖、换号、错误或采样间隔超限时不连接该段。
采样保留缓存及输出比例相近的连续区间，以 pooled token / pooled percentage 计算容量。
每个独立区间保守保留两个百分点的端点不确定性；变化量、记录数或区间不足时显示待采样。
估算只适用于当前负载比例，并同时展示范围、样本记录数、百分点和缓存占比。
本机聚合状态有大小和数量上限，不复制对话正文，不充当 API 计费事实源，也不生成探测提示词。

## Rejected alternatives

- API 单价或固定模型倍率换算订阅余额：没有官方可保证的对应关系。
- 请求发送即提示成功：超时可能发生在服务器已兑换之后。
- 遇错自动换下一张卡：可能重复消耗权益。
- 对混合模型的总百分比变化按 token 数强行分摊：无法确定各模型实际消耗。
- 为统计接管供应商或增加代理：重复 CC Switch 的职责。

## Consequences

需要支持对应权益接口的原生 Codex CLI；不兼容时保留已有额度查询并显示接口不可用。
容量范围需要有效使用积累；日志缺失或未记录速度档位时可能一直处于待采样。
其他设备、未捕获的调用和 ChatGPT Work 等账号活动会影响校准，界面明确该范围不是官方余额。
本轮不自动消费卡片、不兑换付费 credits、不后台发提示词，不扩展手机兑换入口。

## Validation

核对了官方协议、已有 TypeScript 兑换实现、Rust 异步所有权及文件事务调用链。
2026-10-23 Windows debug 产品构建通过（`scripts/build-windows-product.ps1 -Configuration debug`）。
尚未查询个人权益或消耗重置卡；真实接口和原生视觉仍待验收。

## Supersedes

Extends the read-only boundary of [usage adapters](2026-10-23-read-only-usage-adapters.md) only for explicitly confirmed reset redemption and local calibration metadata.
CC Switch billing and configuration ownership is unchanged.

## Revisit when

官方新增稳定的模型 token 余额或事件计量协议、需要系统钥匙串登录、日志格式变更，或需要跨设备归因时。
