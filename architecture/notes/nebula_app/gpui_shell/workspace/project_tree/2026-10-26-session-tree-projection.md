# 会话树投影复用原生标签与分屏身份

## Status

Compiled in Windows GPUI debug build 20261026-133642; 原生交互待用户体验。

## Context

此前将项目文件浏览器挂到左侧，用户纠正目标是会话模块，并选择项目 → 会话 →
分屏/终端，文件与差异标签单独归类。标签仍是运行会话的所有者，不能为了树形
外观再建一套可启动、可关闭或可恢复的会话记录。

## Evidence

- `nebula_app/src/gpui_shell/workspace/project_tree/sessions.rs` 投影标签及分屏。
- `nebula_app/src/gpui_shell/workspace/sidebar.rs` 继续提供原生标签行和菜单。
- `nebula_app/src/gpui_shell/workspace/pane_header.rs` 是分屏视觉顺序与标题来源。
- `nebula_app/src/gpui_shell/workspace/{projects,tab_scroll,details_panel}.rs` 拥有视图状态、激活定位和文件抽屉。

## Decision

以既有 TabMeta runtime ID 标识会话，原生 pane ID 标识终端；点击时重新查找当前
标签并验证 pane 仍存在。分屏顺序与标题复用原 pane_header，状态复用 SidebarActivity。
文件/差异标签单独分组，不计入会话数量。

树使用一个窗口内原生 ScrollHandle，折叠仅为视图状态；原等高行滚动保留给旧侧栏。
树模式不用等高行位移推断拖动排序，避免分屏子行使标签移位。原生跨窗拖放与右键
移动仍保留。文件浏览回到独立右侧抽屉，继续复用唯一 SidePanel 和已有异步生命周期。

## Rejected alternatives

- 把文件树当作会话树：与用户明确选择不符。
- 另建会话身份/排序/运行状态仓库：与恢复、关闭及跨窗操作产生分歧。
- 对展开分支沿用固定行高拖动换位：会把子终端的高度误当其他标签位置。

## Consequences

项目筛选、标题、关闭、重命名和上下文菜单沿用原生所有者。会话树没有新磁盘扫描。
当前项目逐个进入，尚未提供同窗多项目同时展开或树内几何拖动排序。

## Validation

源码回读、格式检查和 20261026-133642 产品编译通过。原生 TabId 是字符串包装类型，
折叠集合使用其 Hash/Eq，回调克隆保留身份；原文件入口的旧折叠分支已移除。
尚未验证真实点击、跨窗拖放、键盘定位与滚动布局。

## Supersedes

`architecture/notes/nebula_app/display/side_panel/2026-10-26-project-explorer-snapshot-owner.md`
中左侧文件浏览的挂载决定。唯一 SidePanel 的数据及异步所有权仍适用。

## Revisit when

需要同窗展开多个项目，或为会话树提供按真实分支几何位置的拖动排序时。
