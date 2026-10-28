# 可视差异与批注共用 patch 行映射

Superseded in part by `architecture/notes/nebula_app/gpui_shell/code_tab/diff/2026-10-26-aligned-comparison.md`：默认改为完整版本左右对照，原补丁锚点保留。

## Status

Implemented in source; 待编译及原生交互验收。

## Context

原预览直接显示 unified patch。增加旧/新行号、增删背景与点选批注时，若另按
编辑器行数推断锚点，新文件、删除行及多个 hunk 会使反馈错位。

## Evidence

- `nebula_app/src/gpui_shell/code_tab/diff/rows.rs` 从原 patch 建立 UTF-8 字节范围与旧/新行号。
- `nebula_app/src/gpui_shell/code_tab/{diff,review}.rs` 共用行映射和原 patch 选区。
- `nebula_app/src/gpui_shell/workspace/review.rs` 保留发送前快照检查。

## Decision

有界 patch 在既有后台读取任务内解析一次供可视列表使用。行选择回写原
InputState 的字节选区；批注也使用相同行映射解释选区，不把可视行索引当文件行号。
保留原始 patch 编辑器作为原生文本选择和复制入口；视图都是只读。

## Rejected alternatives

- 用普通编辑器的递增行号表示文件行号：patch 元数据和 hunk 跳跃使其失真。
- 为不同视图维护独立的差异/批注解析器：增删行定位容易漂移。
- 为界面额外读取完整文件两遍：扩大 I/O 和内存合同，且无必要。

## Consequences

显示统一差异与双行号，支持 Shift 范围选择；不是左右两栏的完整文件比较器。
二进制及没有 hunk 的元数据可查看、复制，不能虚构代码行批注。
既有大小限制、快照失效检查及旧批注保留语义不变。

## Validation

静态回读；待编译及多 hunk、新增/删除文件、Unicode 选区的原生交互验收。

## Supersedes

None. 保留 2026-10-26-review-feedback-snapshots.md 的快照归属。

## Revisit when

增加左右完整文件比较、独立 hunk 暂存或其他 VCS patch 格式时。
