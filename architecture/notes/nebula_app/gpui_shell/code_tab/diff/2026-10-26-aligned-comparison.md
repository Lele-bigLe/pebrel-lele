# 完整版本对照保留原补丁锚点

## Status

Compiled in Windows GPUI debug build 20261026-133642; 原生交互待用户体验。

## Context

用户要求类似 VS Code 的左右差异，而旧列表只显示 unified patch 的旧/新行号。
完整版本的行号、折叠后可见行号与 patch 字节范围不同，不能直接把可见行索引交给
批注或暂存。读取工作区文本和 patch 两次还可能把不同时间的内容拼到一起。

## Evidence

- `nebula_app/src/runtime_api/git/comparison.rs` 复用 desktop_changes 的有界仓库执行器。
- `nebula_app/src/runtime_api/git/patch/{aligned.rs,../patch.rs}` 共用 hunk 语法及字节映射。
- `nebula_app/src/gpui_shell/code_tab/diff/{comparison.rs,../diff.rs}` 拥有异步读取和双栏呈现。
- `nebula_app/src/gpui_shell/code_tab/{review.rs,diff/staging.rs}` 使用原补丁选区及快照。

## Decision

未暂存读取 index 基准，已暂存读取 HEAD 基准；从基准和确切 Git patch 重建修改后
内容，验证 hunk 行数、上下文及行号。读取后复核状态版本和 patch，失效则要求刷新。
新增文件基准为空；重命名使用对应版本路径。文本受既有字节限制及新增行数上限保护。

对齐模型保留真实左右行号、空白占位和可选原 patch 字节范围。未变更长段只在显示
投影中折叠，不改变模型及批注身份。点选/Shift 选区仍写入原 InputState；分块暂存
仍操作选中行所在完整 hunk，不增加逐行写入语义。

两个原生虚拟列表共享显示行及固定高度；prepaint 在列表前同步垂直 offset，分别
保留水平 offset。语法树在既有后台任务中构建，复用安装的高亮器；可见行按当前
主题查询样式。二进制、基准不匹配、冲突或超限保留原始差异及原因，不伪造完整版本。

## Rejected alternatives

- 左右各显示同一份 patch：并非真实修改前后版本。
- 从工作区直接读取第二份文本：增加时序窗口及平台文件读取路径。
- 引入新编辑器或 Git 差异库：已有 Git 执行器、原生列表和高亮依赖足够。
- 按显示行号写暂存或批注：折叠、插入和删除会使目标错位。

## Consequences

保留原始文本选择/复制入口与原 SHA-256 校验。基准读取增加有界后台 Git 调用，
不在 render 中执行 I/O。当前没有行内字符差异、独立逐行暂存或可编辑的双栏内容。

## Validation

源码回读、格式检查和 20261026-133642 产品编译通过。同步滚动、折叠、Unicode 高亮和原生批注/暂存
仍需后续交互验证，不将模型推导视为真实 Git 写入通过。

## Supersedes

`architecture/notes/nebula_app/gpui_shell/code_tab/2026-10-26-visual-patch-anchors.md`
中统一列表呈现限制。原补丁字节锚点、只读语义及快照所有权保留。

## Revisit when

需要行内字符差异、超限文件比较、可编辑 diff 或跨平台逐行暂存时。
