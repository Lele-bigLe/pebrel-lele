# 本机 Worktree 管理复用 Git 权威状态

## Status

Implemented and compiled. 未执行真实 Worktree 创建或界面验收。

## Context

需要给项目提供独立 Git 工作区；既有 WorktreeTransaction 已负责创建和只回滚
自身资源。UI 应复用该能力，避免维护另一个 Worktree 清单或实现删除事务。

## Evidence

- [Worktree 界面](../../../../../../nebula_app/src/gpui_shell/workspace/vcs_panel/worktrees.rs)。
- [共用 Git 执行边界](../../../../../../nebula_app/src/runtime_api/git.rs)。
- [Worktree 创建事务](../../../../../../nebula_app/src/git_worktree.rs)。
- [项目选择与去重](../../../../../../nebula_app/src/gpui_shell/workspace/projects.rs)。

## Decision

列表来自 `git worktree list --porcelain -z`；保留路径字节边界，解析 bare、locked
和 prunable。查询复用桌面差异的执行环境，输出最多 256 KiB，最多展示 256 个
Worktree。读取失败保留错误，不把失败当作空列表。

初版管理 UI 仅对本机 Git 仓库启用；WSL 不把来宾目录交给宿主文件 API。
对话框固定显示打开时的来源仓库，后续会话或侧栏目录变化不改写操作目标。
创建时填写新分支、可选基准和目标目录；来源有未提交改动时必须显式勾选允许，
仍只使用已提交内容。无删除、强制切换或自动清理入口。

创建提交在后台复用事务，成功后转到项目库和会话定位；打开已有项前重新核对
目录并沿用项目选择器，避免创建同项目的第二组元数据。对话框关闭后，已接受的
同步创建事务仍可能完成；结果留在 Git 权威清单，不因 UI 退出删除用户资源。

## Rejected alternatives

- UI 拼接 Shell 命令或复制 branch/worktree 回滚：会产生第二套写入与所有权规则。
- 把源码所在目录的工作区修改自动复制到新 Worktree：混淆提交基线和未提交数据。
- 在 UI 渲染时遍历 Git 或磁盘：阻塞窗口并引入状态时序错误。

## Consequences

Worktree 创建仍继承原事务的 Git 子进程行为；不声称新的 UI 提供进程取消能力。
若创建后目录无法打开，错误明确说明已经创建，用户可刷新 Git 列表恢复定位。
分支切换、stash 和分块暂存不属于这次 Worktree UI 的实现。

## Validation

2026-10-26 Windows GPUI debug 产品构建通过；没有运行真实创建、切分支或删除。

## Supersedes

None.

## Revisit when

需要 WSL 管理、删除/清理、统一写入截止时间或跨操作取消时。
