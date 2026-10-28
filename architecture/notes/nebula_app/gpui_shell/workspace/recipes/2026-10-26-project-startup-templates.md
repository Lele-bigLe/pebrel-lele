# 项目启动模板复用命名布局

## Status

Implemented and compiled. 最终 UI 收尾后已重新编译，运行与视觉待用户验收。

## Context

项目需要可重用的目录、Shell、智能体和分屏组合。已有命名布局与共享 Session
恢复路径，不应增加另一套终端启动或 Agent 身份保存机制。

## Evidence

- [模板存储](../../../../../../nebula_app/src/gpui_shell/workspace/recipes/store.rs) 和
  [启动表单](../../../../../../nebula_app/src/gpui_shell/workspace/recipes/project.rs)。
- [窗口启动与复用](../../../../../../nebula_app/src/gpui_shell/workspace/windowing/recipe.rs)。
- [已有命令队列](../../../../../../nebula_app/src/gpui_shell/terminal/view/startup_command.rs)。
- [已知 CLI 启动拼写](../../../../../../nebula_app/src/ai_agents.rs)。

## Decision

继续使用 recipes 目录、原子文件保存、2 MiB 上限及共享 Session 布局。可选
`startup` 字段区分项目启动模板与旧布局：缺省为旧布局，空数组为仅启动 Shell。
启动指令只存 tab/pane 位置与受支持的工具来源，不存原生对话 ID、任意命令或凭据。

初版仅支持本机目录和检测到的本机 Shell，工具为 Codex/Claude/Gemini/OpenCode。
目录和 Shell 检测、保存与打开前的目录核对放在后台。路径复用既有平台 canonical
函数，避免 Windows verbatim 前缀使同一项目产生不同显示身份。

启动交给原有恢复器及等待 Shell prompt 的命令队列，不把排队当成 Agent 已运行。
同进程内同一项目模板文件定位其已有、仍含终端的窗口，不重放启动指令；旧的
命名布局仍可反复开新窗口。窗口登记中的模板来源只用于运行期定位，不作新数据库。

## Rejected alternatives

- 按标题或目录猜正在运行的原生对话：不能证明身份，会误接续用户任务。
- 保存旧 Agent ID 并自动继续：项目模板应新建任务，恢复属于原生历史和 Session。
- 自建任意 Shell 参数解释器或复制 CC Switch 配置：扩大命令和配置管理范围。

## Consequences

重复定位仅覆盖当前进程；冷恢复没有模板来源映射。新模板暂未纳入现有会话备份
白名单，备份衔接仍是后续任务。模板发起命令后，实际错误由既有终端状态呈现。

## Validation

2026-10-26 Windows GPUI debug 产品构建通过；交互与视觉尚未验收。

## Supersedes

None. 旧布局的去除 Agent 回放和共享恢复路径保持有效。

## Revisit when

需要跨进程模板窗口识别、WSL/SSH 项目模板或参数化命令时。
