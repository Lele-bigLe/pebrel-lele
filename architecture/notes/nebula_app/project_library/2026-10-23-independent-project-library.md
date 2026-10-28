# 项目库独立于终端生命周期

## Status

Windows debug product build passed; native acceptance pending.

## Context

从标签元数据推导项目列表会导致最后一个会话关闭后项目消失，也无法保存名称、分组、收藏和最近打开时间。

## Evidence

- [`projects.rs`](../../../../nebula_app/src/gpui_shell/workspace/projects.rs) 原先只聚合 `TabMeta.project_root`。
- [`atomic_file.rs`](../../../../nebula_app/src/atomic_file.rs) 已有短事务锁、原子替换及进程生存期锁。
- 会话备份使用 [`encrypted_backup.rs`](../../../../nebula_app/src/encrypted_backup.rs) 的明确文件白名单。

## Decision

[`project_library.rs`](../../../../nebula_app/src/project_library.rs) 拥有有版本的 `projects.json`。
终端标签仍拥有各自的项目归属；项目库只保存根路径和用户元数据，不拥有 PTY。
首次载入把已有标签的项目补入库，不改动现有会话文件。
关闭标签不删除项目。选择零会话项目会在该目录新建终端；有可见会话时优先聚焦。

后台事务在文件锁内重新读取后修改并原子写入，短暂锁竞争做有界等待。
文件损坏、未知格式、重复根路径或超出上限时拒绝覆盖并显示错误。
同一进程的工作区按递增 revision 接受投影，旧回调不能覆盖更新的数据。
收藏优先，其后按分组及最近打开排序。原目录保留完整提示，用户名称只改变展示。
本地 Windows 路径忽略大小写；UNC/WSL 路径保留大小写，不对历史路径执行 canonicalize 或 stat。

项目库随用户选择的 Session 备份分类导出，导入时校验格式。
恢复同样取得项目锁，并以原文件和进程已见 revision 的最大值递增，避免恢复旧备份后界面永久拒绝新投影。
撤回保留恢复点的原始字节合同，写回时持有同一项目锁；现有 BackupRestored 事件触发重新加载及 revision 协调。
兑换请求标识与额度采样属于本机运行状态，不加入备份白名单。

## Rejected alternatives

- 把项目当作仍打开的标签：无法保留无会话项目。
- 每次保存都覆盖内存快照：多窗口会丢失其他窗口的修改。
- 启动时探测所有路径：可能唤起 WSL、连接网络盘或阻塞首帧。

## Consequences

项目目录可以暂时离线，启动终端时沿用已有错误处理。
不同 Pebrel 进程的存储事务不会覆盖彼此；另一个进程的界面在下一次库事务后取得最新投影。

## Validation

进行了持久化、筛选、导入白名单和异步调用链的源码回读。
2026-10-23 Windows debug 产品构建通过（`scripts/build-windows-product.ps1 -Configuration debug`）；原生交互仍待用户体验验收。

## Supersedes

None. Extends tab project membership without replacing it.

## Revisit when

需要跨进程即时通知、项目删除、跨平台路径映射或独立项目布局快照时。
