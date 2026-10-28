# 桌面 stash 与同源分块暂存

## Status

Windows GPUI debug 产品构建通过（123503）。 Real Git mutation acceptance pending.

## Context

用户希望集中完成 Git 日常管理与 Agent 修改审阅。分块写入必须对应实际看到的
差异，stash 编号变化不能将恢复操作指向另一条记录。

## Evidence

- `runtime_api/git/patch.rs` 接管已有可视行映射；差异显示、批注和分块暂存共用它。
- `desktop_changes.rs` 复用 Repository 的字面路径、25 秒期限及 256 KiB 输出预算。
- 本机 Git 帮助确认 stash show 的 include-untracked、apply 的 index 参数。

## Decision

Stash 列表最多 256 条，截断有提示；差异只读。创建和 apply 复用 SidePanel
单一 VCS 写队列，派发时确认仍是来源 GitLocation。恢复使用完整对象 ID，
可选恢复 index，保留 stash；没有 pop/drop/clear。创建明确说明会移走已暂存及
未暂存改动，可选包括未跟踪文件，不包括忽略文件。Stash/Worktree 收入 Git 工具菜单。

分块暂存限本机已跟踪的普通文本修改，选择块内任意行会暂存整个 hunk。
UI 的字节选区和后台校验用同一解析器。后台重读来源文件的当前差异，检查完整
SHA-256 与所见快照相同，提取原文件头及完整 hunk；先 apply --cached --check，
再 apply --cached。临时 patch 由既有 tempfile 在应用私有目录持有并回收。

进程内复用仓库锁，实际写 index 仍由 Git 自身锁和上下文校验处理。失败使旧
快照失效，刷新前不重试；成功后重读差异，两种结果均请求刷新面板。工作区
文件不因分块暂存改写，未向手机协议增加写入口。

## Rejected alternatives

- 以 stash@{n} 执行恢复：另一窗口可能改变编号。
- 为暂存维护第二套 hunk 解析器或根据可视行号重建 patch：容易与批注定位漂移。
- 对二进制、重命名、模式变化及新建/删除文件推断部分补丁：改用既有整文件暂存。

## Consequences

WSL 当前保留整文件暂存；无左右完整文件比较器。外部 Git 仍可能并发操作，
原生错误和超时不承诺回滚或未执行。冲突通过原 Git 面板处理，stash 保留。

## Validation

核心功能阶段产品构建通过；完整合并构建结果见产品清单。未在用户仓库执行
stash、暂存、恢复、切分支或提交，也未新增自动测试。

## Supersedes

更新 `gpui_shell/code_tab/2026-10-26-visual-patch-anchors.md` 的解析器落点，
扩展其仅只读差异视图的交互；原文件快照与审阅归属合同不变。

## Revisit when

需要逐行暂存、反向分块取消暂存、WSL patch 输入或 stash 删除时。
