# 跨窗口目标和新建智能体保留明确发送边界

## Status

Implemented in source; 待编译及真实会话交互验收。

## Context

旧选择器只枚举当前窗口的前台 Agent，没有目标即退出。用户主要使用 Codex，
希望可选接收者并能在没有现成会话时新建。

## Evidence

- `nebula_app/src/gpui_shell/workspace/send_to_chat.rs` 选择器保存明确目标的弱引用。
- `nebula_app/src/gpui_shell/workspace/send_to_chat/pending.rs` 拥有新建后的窗口内待发送反馈。
- `nebula_app/src/gpui_shell/terminal/view/runtime.rs::runtime_chat_message` 保留真正输入的强制校验。

## Decision

使用窗口注册表枚举其他窗口，跳过当前正在更新的 workspace，避免重入借用。
只收集当前前台 Agent，不用历史身份冒充运行态；窗口、项目和远端身份在选择时可见。
默认同项目本机 Codex，否则可在来源本机项目新建常用 CLI。

新建只调用既有终端启动流程。反馈保留在所属窗口的待发送栏；登录、仓库信任
或其他交互由用户完成，然后再次点击发送。没有定时器向启动/批准提示注入文本。
真正发送仍校验前台工具、session、bracketed paste 等；拒绝时保留内容供复制。

## Rejected alternatives

- 从系统进程或标题推断任意外部终端可接收输入：没有受控 pane 身份。
- 对刚启动的 CLI 自动重复粘贴直到成功：可能落入登录、信任或其他提示。
- 用强引用让已关闭 pane 继续存活：混淆会话关闭与接收者生命周期。

## Consequences

待发送反馈是窗口内临时状态，取消或关窗后不持久化。新建 CLI 不代表 CLI 已就绪。
外部应用内的 Agent 不属于此选择器；纯 WSL 审阅可选已有目标，新建入口仅用于本机项目。

## Validation

静态回读；尚未启动 CLI 或进行真实反馈发送。

## Supersedes

None. 扩展 Send to Chat，不新增模型调用和供应商配置。

## Revisit when

CLI 提供可证明就绪的协议、需要持久化待发消息或跨进程目标时。
