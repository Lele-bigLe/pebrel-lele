# 项目文件树与 Git 使用同一个 SidePanel 数据所有者

Superseded in part by `architecture/notes/nebula_app/gpui_shell/workspace/project_tree/2026-10-26-session-tree-projection.md`：左侧改为会话树，文件浏览恢复独立抽屉；数据所有权保留。

## Status

Implemented in source; 待编译及原生交互验收。

## Context

用户要求同项目的会话和文件放到左侧树中，同时保留 Git 审阅。原文件树和 Git
共享 SidePanel，过去切到 Git 会清空文件行和查询；直接同时画两处会导致左侧空白。

## Evidence

- `nebula_app/src/display/side_panel/mod.rs` 拥有目录快照、搜索世代和后台结果收割。
- `nebula_app/src/gpui_shell/workspace/{project_tree,details_panel}.rs` 拥有可见性、项目绑定及轮询生命周期。
- `nebula_app/src/gpui_shell/workspace/{sidebar,file_tree}.rs` 复用既有会话和文件行交互。

## Decision

只增加 files_visible 消费者可见性，保留一个目录/搜索模型。Git 可见时不清空
仍在左侧显示的文件行；两个表面都关闭后停止 UI 轮询。工人仍以原根目录、
WSL 身份和搜索世代检查过期结果，不增加文件系统定时扫描。

本机项目模式绑定项目归属根，明确的树内浏览仍使用原 custom root。项目概览
和单项目展开由既有项目库/TabMeta 驱动；折叠状态仅在窗口内保留。纯 WSL、
SSH、顶部标签及文档详情保留原呈现入口，不把远端路径交给宿主目录读取器。

## Rejected alternatives

- 再建一个文件模型或后台索引：产生两份选择、快照和查询状态。
- 渲染时临时把 view 改成 Files：异步查询和 Git 行为会读到错误模式。

## Consequences

当前单项目展开，其他项目由概览或选择器切换；没有增加多根并行文件索引。
文件刷新继续沿用显式刷新及既有事件触发。

## Validation

静态回读；待编译、跨项目/视图切换和原生布局验收。

## Supersedes

None. 扩展可见消费者，不替换既有数据/搜索所有权。

## Revisit when

需要同窗同时展开多个仓库文件树，或改变文件自动刷新策略时。
