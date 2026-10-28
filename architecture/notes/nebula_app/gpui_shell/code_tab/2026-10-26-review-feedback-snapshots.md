# 审阅批注使用差异快照并复用发送入口

## Status

Implemented in source; 尚未构建及原生交互验收。

## Context

用户需要审阅 Agent 修改结果并把多条意见发回原会话。既有 Git diff 只读查看器、
Send to Chat 目标选择及 bracketed paste 已有完整身份和输入边界，应继续复用。

## Evidence

- `nebula_app/src/gpui_shell/code_tab/{diff,review}.rs` 拥有显示的 patch 和文件批注。
- `nebula_app/src/gpui_shell/workspace/review.rs` 拥有同仓库批次检查的后台任务。
- `nebula_app/src/gpui_shell/workspace/send_to_chat.rs` 拥有明确目标选择及真正发送。

## Decision

批注是视图拥有的临时草稿，不复制完整对话，不新增模型调用或持久化库。
文件与行上下文来自当前显示的 unified diff；原始片段作为引用发送。每个差异快照
在后台读取时计算 SHA-256，批注固定到该快照；刷新后变动的批注不能直接移接新行。

发送按 GitLocation 汇总当前已打开文件的批注，区分 index/worktree。后台逐项重读
diff，检查摘要；回到 UI 后再次检查视图仍存在、草稿未改。失效批注阻止整个批次，
不静默丢弃。共享 chat message 校验通过后，打开既有目标选择及预览对话框。
只有用户确认目标并发送才写入目标 Agent；取消或拒绝时保留批注。

发送的是带快照语境的审阅意见，不是可直接套用的补丁。预览期间文件仍可能变化，
消息要求接收者先核对当前文件。工作区差异也不被冒称为某个 Agent 的本轮修改。

## Rejected alternatives

- 按编辑器行号绑定后无条件跟随新 patch：新增/删除行会造成意见错位。
- 为反馈另建模型/供应商接口：重复配置职责和输入安全合同。
- 检查成功即清空草稿或声称发送成功：后续用户仍可能取消，目标也可能拒绝。

## Consequences

批注有数量/体积限制，当前仅随打开的差异视图保留；关闭视图不持久化草稿。
目标仍须是可接收消息的 Agent，远端发送遵循既有用户选择规则。

## Validation

源码核对；未运行构建、测试或真实会话发送。

## Supersedes

None. 扩展只读 diff 交互，不改变底层 Git 写入或发送合同。

## Revisit when

需要持久化草稿、精确任务归属或跨窗口批次时。
