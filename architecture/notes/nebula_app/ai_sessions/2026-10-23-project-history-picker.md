# 按项目和工具浏览历史会话

## Status

Implemented in source; CLI resume and native UI acceptance pending.

## Context

用户选择本轮仅支持项目/工具筛选、搜索及继续会话。已有 Quick Jump 的最近
30 条恢复入口不足以浏览历史；也不应复制聊天正文或建立另一份可写会话数据库。

## Evidence

- `ai_sessions.rs` 已拥有 Claude/Codex 原生扫描与 Hook 注册索引合并。
- 本机 OpenCode 的只读 `session` 表包含标题、目录、更新时间和父会话/归档字段。
- 本机 Gemini 保存 `tmp/<project>/chats/session-*.json`；`projects.json` 将原项目
  目录映射到存储目录名，聊天记录含 sessionId 和 messages。
- Codex 原生 session_index.jsonl 含 id、thread_name、updated_at，可恢复 CLI 命名。
- AgentKind 已统一验证并构建四类工具的继续会话命令。

## Decision

`scan_history` 增加原生 OpenCode/Gemini 索引，仍复用单一合并、去重、排序逻辑。
OpenCode 用只读 SQLite 展示主会话（包括归档），排除子代理；Gemini JSON 单文件最多
8 MiB，超限或无效文件跳过。Claude 从已有有界头部补取真实 cwd；Codex 遵守
现有 codex_home，并读取命名索引尾部最多 2 MiB，按原生更新时间选择标题，
避免大段注入上下文把历史列表变成 UUID。保留 Quick Jump 的原有轻量扫描，
不在其 UI 回调解析大 JSON 或重建命名索引。

新历史入口复用命令面板的搜索、键盘和执行链。扫描在面板拥有的后台任务执行，
最多显示最近 500 条；关闭或换入口取消等待。项目按路径组件包含子目录，工具
按类型筛选，搜索标题、目录和工具。项目名称复用最短唯一后缀及完整路径 tooltip。
恢复时沿用所选项目或现有最长匹配项目根，避免被当前无关项目吸收。

浏览期间不 stat 历史项目目录，避免 UNC/WSL 访问启动来宾；明确恢复时由终端
启动路径报告目录不可用。无法识别原目录时不把命令发送到默认目录。恢复命令
继续由 AgentKind 验证，未增加重命名、收藏、删除或转录正文存储。

## Rejected alternatives

- 独立历史窗口与输入实现：复制已有键盘和焦点所有权。
- 渲染时同步扫盘或验证所有目录：大档案和网络路径可能阻塞 UI 或启动来宾。
- 根据 Claude 编码目录名逆推 cwd：编码不可逆，可能进入错误项目。

## Consequences

历史依赖本地工具真实存储；不覆盖 Gemini 新版 JSONL 或旧版 OpenCode JSON
目录。缺少映射、无效格式或大于上限的记录不承诺可恢复。筛选限于最近 500 条。

## Validation

只核对了本机索引结构、字段和 Gemini 项目映射方向，未输出历史对话或原生 ID。
完成源码格式化、架构静态检查和中英文 JSON 解析；未执行继续会话或新产品构建。

## Supersedes

None.

## Revisit when

需要分页浏览全量历史、更多存储格式、跨设备索引或用户要求的写入管理功能时。
