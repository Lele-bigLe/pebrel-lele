use super::*;
use crate::gpui_shell::widgets::toolbar_button;
use crate::i18n::Message;

/// 折叠箭头的固定布局槽。图标是 SVG，不应借任一字体的 advance 决定留白。
const TABS_DISCLOSURE_SLOT_W: f32 = 24.0;

/// `SidebarActivity::WaitingInput` / `Attention` 共用的字位：Nerd Font
/// `nf-fa-hand_paper_o`（开掌）。已核对打包字体
/// `assets/fonts/MapleMonoNormal-NF-CN-Regular.ttf` 的 cmap 覆盖这个码位；换字体
/// 前先核，否则会掉成豆腐块。
const WAITING_INPUT_GLYPH: &str = "\u{f256}";

/// 侧栏新建入口的生产命中区；鼠标测试直接复用，避免只验证一个近似按钮。
pub(super) fn sidebar_new_tab_control(
    header_group: SharedString,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    h_flex()
        .id("sidebar-new-tab")
        .size(px(SIDEBAR_PLUS_SIZE))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .invisible()
        .group_hover(header_group, |button| button.visible())
        // 标题整行也可点击折叠；必须在按下阶段截断，不能等 Click
        // 才处理，否则父行已经收到同一次起手。
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(on_click)
}

/// 事件 vs 状态：一个 tab 此刻该显示哪个静息徽章。
///
/// - `Done` 是**事件**——「回合完成了，你不在场」。你正看着这个 tab 时它没有
///   信息量了，还画一个点等于让用户去点掉自己刚亲眼看完的事，所以当前 tab
///   一律降回 `Idle`。
/// - 后台响铃补位到 `Done` 同理，只对非当前 tab 成立。`has_bell` 本身激活即清
///   （见 `TabMeta::has_bell`），这里再挡一次是因为清除发生在激活事件里，同一帧
///   内读到的可能还是旧值。
/// - 其余都是**状态**（转圈、等输入、等授权、失败），描述「此刻仍然如此」，你看
///   不看它都还成立，所以当前 tab 也照画。响铃补位的 `Idle` 前提保证了它不会
///   盖掉这些。
pub(super) fn resting_activity(
    activity: SidebarActivity,
    active: bool,
    has_bell: bool,
) -> SidebarActivity {
    match activity {
        SidebarActivity::Done | SidebarActivity::Completed if active => SidebarActivity::Idle,
        SidebarActivity::Idle if !active && has_bell => SidebarActivity::Done,
        other => other,
    }
}

impl NebulaWorkspace {
    pub(super) fn shell_status_label(
        tag: SharedString,
        family: SharedString,
        size_px: f32,
        color: gpui::Hsla,
    ) -> gpui::Div {
        // As in 1.7, short labels keep their intrinsic width and align right in
        // the status row. Only a long label is clipped to the available space.
        div()
            .max_w_full()
            .min_w_0()
            .truncate()
            .font_family(family)
            .text_size(px(size_px))
            .font_weight(FontWeight::NORMAL)
            .text_color(color)
            .child(tag)
    }

    pub(super) fn tab_status_slot(width: f32) -> gpui::Div {
        // Nerd Font ink can extend beyond its advance. Keep the 1.7 slot
        // unclipped; the enclosing tab still clips at its own outer boundary.
        div().relative().w(px(width)).h_full().flex_shrink_0()
    }

    /// Reserve the shaped shell label's width, capped to leave room for the title.
    pub(super) fn shell_status_width(
        window: &Window,
        tag: Option<&SharedString>,
        family: &SharedString,
        size_px: f32,
        minimum: f32,
        maximum: f32,
    ) -> f32 {
        let Some(tag) = tag else { return minimum };
        let line = window.text_system().shape_line(
            tag.clone(),
            px(size_px),
            &[gpui::TextRun {
                len: tag.len(),
                font: gpui::font(family.clone()),
                color: gpui::Hsla::default(),
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        );
        f32::from(line.width).ceil().clamp(minimum, maximum.max(minimum))
    }

    /// 静态细轨道加 30% 的旋转弧。SVG 的缓存身份只含路径和像素尺寸，
    /// 每帧复用 alpha mask，仅更新矩阵；不重新生成路径或逐点混色。
    pub(super) fn spinner(
        phase: f32,
        track: gpui::Rgba,
        head: gpui::Rgba,
        workspace: gpui::WeakEntity<Self>,
    ) -> impl IntoElement {
        canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                // 两处 status 容器的 hover 都用 invisible()：GPUI 的 Hidden
                // 分支跳过子元素 paint，根本不会走到这里。折叠/滚动则由
                // 当前 content mask 裁定；不要改成仍会调用 paint 的 opacity(0)。
                if bounds.intersect(&window.content_mask().bounds).is_empty() {
                    return;
                }
                let _ = workspace.update(cx, |workspace, cx| {
                    workspace.spinner_visible.set(true);
                    workspace.arm_activity_spinner_frame(window, cx);
                });
                // SVG mask 按宽度等比栅格化；吸附后仍须保持物理像素正方形，
                // 否则 16×17 的边界会让实际 sprite 中心与旋转中心错开。
                let side = window.pixel_snap(bounds.size.width.min(bounds.size.height));
                let origin =
                    window.pixel_snap_point(bounds.center() - gpui::point(side / 2.0, side / 2.0));
                let bounds = Bounds::new(origin, size(side, side));
                let center = bounds.center();
                let scale = window.scale_factor();
                let transform = gpui::TransformationMatrix::unit()
                    .translate(center.scale(scale))
                    .rotate(gpui::radians(phase * std::f32::consts::TAU))
                    .translate(center.scale(-scale));
                for (path, matrix, color) in [
                    ("icons/nebula-spinner-track.svg", gpui::TransformationMatrix::unit(), track),
                    ("icons/nebula-spinner-arc.svg", transform, head),
                ] {
                    if let Err(error) =
                        window.paint_svg(bounds, path.into(), None, matrix, color.into(), cx)
                    {
                        log::warn!("cannot draw activity spinner: {error}");
                    }
                }
            },
        )
        .size(px(11.0))
    }

    /// 「有人在等你动手」的徽章：使用 Nerd Font 开掌 `nf-fa-hand_paper_o`
    /// （U+F256）。`WaitingInput`（命令停在交互提示）与 `Attention`（agent 卡在
    /// 授权/提问）共用这一个形状，因为它们要求用户做的事是同一件。
    ///
    /// 用**字体字形**而不是旧壳 `display::ui::icons::push_hand`：那一版是手推
    /// quad、五指靠挖空撑形，小尺寸下指缝糊成一团读不出「手」，用户 08-02 判掉
    /// 了（`display/chrome.rs` 里那段注释和被注掉的调用还留着）。这个码位在打包
    /// 字体 `MapleMonoNormal-NF-CN-Regular.ttf` 里是字体设计师画的描边轮廓，
    /// 指缝属于轮廓本身、有正经笔重，缩到徽章尺寸不塌——和被否掉的那版不是同
    /// 一种东西。
    ///
    /// 形状表达语义、颜色表达严重度：停在 `[y/n]` 是常态，取 `primary`；agent
    /// 卡在授权上会把整个回合堵住，取 `warning`。警示形状（⚠ 三角）留给失败，
    /// 不能拿去表示「问你一句」。
    pub(super) fn waiting_hand(
        family: SharedString,
        size_px: f32,
        color: gpui::Hsla,
    ) -> impl IntoElement {
        div()
            .font_family(family)
            .text_size(px(size_px))
            .font_weight(FontWeight::NORMAL)
            .text_color(color)
            .child(WAITING_INPUT_GLYPH)
    }

    pub(super) fn sidebar_activity_badge(
        &self,
        activity: SidebarActivity,
        active: bool,
        family: SharedString,
        label_px: f32,
        cx: &Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let theme = cx.theme();
        Some(match activity {
            SidebarActivity::Running => {
                let (track, head) = crate::gpui_shell::theme::sidebar_spinner_colors(cx, active);
                Self::spinner(self.spinner_phase, track, head, cx.weak_entity()).into_any_element()
            },
            SidebarActivity::Paused => {
                Icon::new(IconName::Pause).xsmall().text_color(theme.warning).into_any_element()
            },
            SidebarActivity::Done => {
                div().size(px(6.0)).rounded_full().bg(theme.primary).into_any_element()
            },
            SidebarActivity::Completed => {
                Icon::new(IconName::Check).xsmall().text_color(theme.success).into_any_element()
            },
            SidebarActivity::CommandFailed => Icon::new(IconName::TriangleAlert)
                .xsmall()
                .text_color(theme.danger)
                .into_any_element(),
            SidebarActivity::Failed => {
                Icon::new(IconName::CircleX).xsmall().text_color(theme.danger).into_any_element()
            },
            // Ordinary shell input and blocked agent approval share a shape, not severity.
            SidebarActivity::WaitingInput | SidebarActivity::Attention => Self::waiting_hand(
                family,
                label_px,
                if activity == SidebarActivity::Attention { theme.warning } else { theme.primary },
            )
            .into_any_element(),
            SidebarActivity::Idle => return None,
        })
    }

    pub(super) fn session_status_dot(
        &self,
        activity: SidebarActivity,
        is_agent: bool,
        id: SharedString,
        cx: &Context<Self>,
    ) -> Option<gpui::AnyElement> {
        if activity == SidebarActivity::Idle && !is_agent {
            return None;
        }
        let theme = cx.theme();
        let (color, message) = match activity {
            SidebarActivity::Running => (theme.primary, Message::SessionStatusRunning),
            SidebarActivity::Paused => (theme.muted_foreground, Message::SessionStatusPaused),
            SidebarActivity::Done | SidebarActivity::Completed => {
                (theme.success, Message::SessionStatusCompleted)
            },
            SidebarActivity::WaitingInput => (theme.warning, Message::SessionStatusWaitingInput),
            SidebarActivity::Attention => (theme.warning, Message::SessionStatusAttention),
            SidebarActivity::CommandFailed | SidebarActivity::Failed => {
                (theme.danger, Message::SessionStatusFailed)
            },
            SidebarActivity::Idle => (theme.muted_foreground, Message::SessionStatusIdle),
        };
        let label = crate::gpui_shell::config::ui_language(cx).text(message);
        Some(
            div()
                .id(id)
                .role(gpui::Role::Status)
                .aria_label(label)
                .size(px(6.0))
                .flex_shrink_0()
                .rounded_full()
                .bg(color)
                .tooltip(move |window, cx| Tooltip::new(label).build(window, cx))
                .into_any_element(),
        )
    }

    fn render_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        if self.workbench_changes(cx) {
            return self.render_workbench_changes_sidebar(window, cx);
        }
        if matches!(self.projects.filter, super::projects::ProjectFilter::All)
            && !self.project_entries().is_empty()
        {
            return self.render_projects_overview(cx);
        }
        let tree_mode = self.project_tree_mode(cx);
        let language = crate::gpui_shell::config::ui_language(cx);
        let row_height = tab_scroll::tab_row_height(self.density);
        let row_pitch = self.tab_row_pitch();
        let project_switcher = self.render_project_switcher(cx).into_any_element();
        let theme = cx.theme().clone();
        let muted = theme.muted_foreground;
        // 数量 chip 数字：旧壳独用 ink_faint（比 ink_dim 再暗一档），chip 背
        // 景 surface 洗色不变——数字不该和标题抢层级。
        let faint = crate::gpui_shell::theme::faint_ink(cx);
        let active_bg = theme.sidebar_accent;
        let active_fg = theme.sidebar_accent_foreground;
        let hover_bg = theme.list_hover;
        let dark = theme.is_dark();
        // 标题走 UI sans，路径/状态短标才用 Normal 等宽；有图标时才占图标槽。
        let settings = cx.try_global::<crate::gpui_shell::config::Settings>();
        let tab_close_visible = settings.map(|settings| settings.tab_close_visible).unwrap_or(true);
        let tab_reveal = settings
            .map(|settings| settings.tab_reveal)
            .unwrap_or(nebula_settings::TabRevealName::Slide);
        let chrome_family = theme.font_family.clone();
        let metadata_family = theme.mono_font_family.clone();
        let symbol_family: SharedString = crate::font_install::REQUIRED_FONT_FAMILY.into();
        // 界面字号独立于终端缩放。
        let label_px = settings.map(|settings| settings.ui_font_size_px).unwrap_or(15.0);

        // 受约束拖拽的渲染参数：激活后被拖行骑指针位移，落点槽位由位移换算。
        let drag = self
            .tab_drag
            .as_ref()
            .filter(|d| d.active && d.axis == TabDragAxis::Vertical)
            .map(|d| (d.source, self.tab_drag_target(d), d.offset));

        // 折叠只裁剪槽位，不改窗口算法：旧壳 `tabs_avail` 与 `tabs_open`
        // 分开——折起来时行矩形为零，但可用高度仍按面板剩余算。
        let (tabs_scroll, tabs_show) = self.tabs_visible_window();
        // 行的确定宽度：侧栏宽 − 侧栏 p_2 两边 − 列表右侧滚动条留白。
        // 与下面 `label_avail` 同一份减法口径，两者不能各算一套。
        let row_w = (self.sidebar_width
            - 16.0
            - if tree_mode { 0.0 } else { tab_scroll::TAB_SCROLL_GUTTER })
        .max(1.0);
        let mut indices = self.sidebar_tab_indices();
        if tree_mode {
            indices.sort_by_key(|&index| !self.tabs[index].is_terminal());
        }
        let first_document = indices.iter().copied().find(|&index| !self.tabs[index].is_terminal());
        let items = indices
            .iter()
            .copied()
            .enumerate()
            .filter(|&(position, ix)| {
                if tree_mode {
                    if self.tabs[ix].is_terminal() { true }
                    else { !self.projects.documents_collapsed || Some(ix) == first_document }
                } else { tab_scroll::index_visible(position, tabs_scroll, tabs_show) }
            })
            .map(|(_, ix)| {
                let row_w = if tree_mode && !self.tabs[ix].is_terminal() { row_w - project_tree::TREE_INDENT } else { row_w };
                let active = ix == self.active;
                let session_row = tree_mode && self.tabs[ix].is_terminal();
                let icon_width = if tree_mode { project_tree::TREE_SLOT } else { TAB_LABEL_ICON_W };
                let TabPresentation {
                    title,
                    tooltip,
                    is_settings,
                    activity,
                    logo_image,
                    logo_pending,
                    program_glyph,
                    shell_tag,
                    color: tab_color,
                    renaming,
                    pane_count,
                    local_administrator,
                } = self.tab_presentation(ix, cx, dark);
                let expanded_session = tree_mode && pane_count > 1
                    && self.projects.expanded_sessions.contains(&self.meta(ix).runtime_id);
                let tree_agent = tree_mode.then(|| self.tabs[ix].focused_view()
                    .and_then(|view| view.read(cx).session_agent())).flatten();
                let tool = session_row.then(|| {
                    tree_agent.as_ref().map_or("Shell", |agent| {
                        if agent.source == "claude" { "Claude" } else {
                            crate::ai_agents::AgentKind::parse(&agent.source)
                                .map_or(agent.source.as_str(), |kind| kind.display_name())
                        }
                    }).to_owned()
                });
                let activity = if tree_mode {
                    self.tabs[ix].focused_view().map(|view| view.read(cx).sidebar_activity()).unwrap_or(activity)
                } else { activity };
                let selected = active && !expanded_session;
                let active_fg = if expanded_session && active { theme.foreground } else { active_fg };
                let hover_group: SharedString = format!("sidebar-tab-hover-{ix}").into();
                let has_program_glyph = program_glyph.is_some();
                let status_width = if tree_mode { ((label_px * 1.18).max(15.0) * 1.25 + 10.0).max(32.0) } else { Self::shell_status_width(
                    window,
                    shell_tag.as_ref(),
                    &metadata_family,
                    label_px * SIDEBAR_TAG_SCALE,
                    TAB_STATUS_SLOT_W,
                    row_w * 0.45,
                ) };
                let cross_window_drag = self.cross_window_drag_payload(ix, cx);
                // 用户明确设置过的标签色：行左侧一条竖光条（上下各留 7、宽 2.5，
                // 与旧壳 strip 同源）。默认标签不占这层视觉层级。
                //
                // 光条贴住行左缘，不沿用旧壳的左内缩 4：旧壳内容起点是 14px，这里
                // 是 `px_2` = 0.5rem，rem 跟随界面字号（默认 14 → 7px，最小 10 →
                // 5px）。内缩会让光条贴上图标；贴边时间隙就是内边距减光条宽，随
                // 字号一起缩放。
                let strip = tab_color.map(|color| gpui::Rgba {
                    r: color.r as f32 / 255.0,
                    g: color.g as f32 / 255.0,
                    b: color.b as f32 / 255.0,
                    a: 1.0,
                });
                let status_color = if active { active_fg } else { muted };
                let resting_status = if session_row {
                    Some(h_flex().gap_1().items_center().justify_end()
                        .when(pane_count > 1, |row| row.child(pane_header::split_badge(pane_count, label_px, status_color, theme.muted)))
                        .children(self.session_status_dot(activity, tree_agent.is_some(),
                            format!("session-status-{}", self.meta(ix).runtime_id.0).into(), cx))
                        .into_any_element())
                } else { self.sidebar_activity_badge(
                    activity, selected, symbol_family.clone(), label_px, cx,
                ).or_else(|| shell_tag.map(|tag| {
                        Self::shell_status_label(
                            tag,
                            metadata_family.clone(),
                            label_px * SIDEBAR_TAG_SCALE,
                            status_color,
                        )
                        .into_any_element()
                    })) };
                // 三类行位移（旧壳 tab_drag_draw_y 的语义）：被拖行骑指针，
                // 源与落点之间的行向反方向让一个槽位，其余不动。存储顺序在
                // 拖拽期间不变，释放时一次性提交。
                let (dragged, shift) = match drag {
                    Some((src, _, _)) if ix == src => (true, 0.0),
                    Some((src, tgt, _)) if src < tgt && ix > src && ix <= tgt => {
                        (false, -row_pitch)
                    },
                    Some((src, tgt, _)) if src > tgt && ix >= tgt && ix < src => {
                        (false, row_pitch)
                    },
                    _ => (false, 0.0),
                };
                let row = h_flex()
                .id(("sidebar-tab", ix))
                .debug_selector(move || format!("sidebar-tab-{ix}"))
                .when_some(tooltip, |row, text| row.tooltip(move |window, cx| {
                    tab_presentation::tooltip(text.clone(), window, cx)
                }))
                .group(hover_group.clone())
                .relative()
                // 旧壳 `layout.tabs[i]` 的命中矩形覆盖整条可见行。
                //
                // 这里必须是**显式像素宽**，不能用 `w_full`：行在
                // `tab_scroll::wrap_tabs_scroll_list` 的 overflow_hidden 容器
                // 里，百分比宽度到不了这一层，会回落成 shrink-to-fit——表现
                // 就是行宽跟着文件名长短变，带图标的行还整体右移一个图标宽
                // （用户 08-19 报的侧栏 tab 宽度乱跳）。双击重命名"看起来正常"
                // 只是因为 Input 恰好把容器撑满，不是宽度真的对了。
                .w(px(row_w))
                // 内容再宽也不许把行撑开：截断后的标题若比测量值宽一两像素，
                // 撑开的行会重新引入上面那个症状。
                .overflow_hidden()
                .gap_2()
                .px_2()
                .when(tree_mode, |row| row.gap(px(8.0)).px(px(10.0)).h(px(36.0)))
                .h(px(if tree_mode { 36.0 } else { row_height }))
                .items_center()
                // 旧壳 pill 圆角 = UI_CORNER_RADIUS_LOGICAL(8)，rounded_md(6)
                // 偏小一圈，选中水洗的轮廓形状会不一样。
                .rounded(px(crate::display::UI_CORNER_RADIUS_LOGICAL))
                // GPUI 默认文本样式可能把侧栏整行带到中等/粗体；旧壳
                // tab chrome 使用终端 Regular face，所有子文本从这里继承常规字重。
                .font_weight(FontWeight::NORMAL)
                .cursor_pointer()
                .when(active, |item| item.text_color(active_fg).when(!tree_mode, |row| row.bg(active_bg)))
                .when(!active && !dragged, |item| {
                    item.text_color(muted).when(!tree_mode, |row| row.hover(|style| style.bg(hover_bg)))
                })
                .when(!active && dragged, |item| item.text_color(muted).bg(hover_bg))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.activate_tab(ix, window, cx);
                }))
                // 受约束拖拽：按下待命（源下标 + 按点 Y），移动阈值与让位
                // 由 update_tab_drag 驱动；激活后的指针独占见 render 根部
                // 的透明罩层。
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                        if this.project_tree_mode(cx) { return; }
                        this.tab_drag = Some(TabDrag {
                            source: ix,
                            cross_window: this.cross_window_drag_payload(ix, cx),
                            cross_window_target: None,
                            press_x: f32::from(event.position.x),
                            press_y: f32::from(event.position.y),
                            axis: TabDragAxis::Vertical,
                            pitch: row_pitch,
                            offset: 0.0,
                            active: false,
                            dock: None,
                        });
                    }),
                )
                .on_mouse_down(
                    MouseButton::Middle,
                    cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.request_close_tab(ix, window, cx);
                    }),
                )
                .on_double_click(cx.listener(move |this, _, window, cx| {
                    // 旧壳 `ChromeHit::Tab` + DoubleClick → BeginRename。
                    cx.stop_propagation();
                    this.begin_rename(ix, window, cx);
                }))
                .when_some(strip, |row, color| {
                    row.child(
                        div()
                            .absolute()
                            .left_0()
                            .top(px(7.0))
                            .w(px(2.5))
                            .h(px(row_height - 14.0))
                            .rounded_full()
                            .bg(color),
                    )
                })
                // 权限不被 AI 图标替换；树模式移到行尾，保留固定身份列。
                .when(local_administrator && !tree_mode, |row| {
                    row.child(tab_presentation::administrator_badge(
                        format!("sidebar-admin-{ix}").into(),
                        cx,
                    ))
                })
                // 行首图标的优先级：**先身份、后形态**。AI 品牌图 / 程序字位
                // 表达「这个 tab 里在跑什么」，它必须跟随聚焦 pane；2×2
                // 分屏标记只在没有身份可显示时补位。
                // 反过来（分屏就一律画 grid）会让 claude 分屏之后标签上再也
                // 看不出跑着 claude——用户 08-23 报的「侧边 tab 不跟随激活
                // tab 变化」就是这个。数量由尾部胶囊表达，不必和图标抢槽位。
                .when(tree_mode && pane_count > 1, |row| {
                    let id = self.meta(ix).runtime_id;
                    row.child(Button::new(SharedString::from(format!("session-disclosure-{}", id.0)))
                        .ghost().xsmall().w(px(icon_width)).h(px(28.0)).p_0().flex_shrink_0()
                        .icon(if expanded_session { IconName::ChevronDown } else { IconName::ChevronRight })
                        .aria_label(language.text(Message::SessionTreePanes))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.projects.expanded_sessions.retain(|id| this.tab_meta.iter().any(|meta| meta.runtime_id == *id));
                            if !this.projects.expanded_sessions.remove(&id) {
                                this.projects.expanded_sessions.insert(id.clone());
                            }
                            cx.notify();
                        })))
                })
                .when(is_settings, |row| {
                    row.child(
                        div().w(px(icon_width)).flex_shrink_0().flex().justify_center().child(
                            Icon::new(IconName::Settings)
                                .small()
                                .text_color(if active { active_fg } else { muted }),
                        ),
                    )
                })
                .when_some(logo_image.clone().filter(|_| !tree_mode || pane_count <= 1), |row, image| {
                    row.child(
                        div().w(px(icon_width)).flex_shrink_0().flex().items_center().justify_center().child(
                            img(image).size(px(TAB_LABEL_ICON_SIZE)).object_fit(ObjectFit::Contain),
                        ),
                    )
                })
                .when_some(program_glyph.filter(|_| !tree_mode || pane_count <= 1), |row, glyph| {
                    row.child(
                        div()
                            .w(px(if tree_mode { icon_width } else if logo_pending { TAB_LABEL_ICON_SIZE } else { TAB_LABEL_ICON_W }))
                            .flex_shrink_0()
                            .when(tree_mode, |slot| slot.flex().justify_center())
                            .font_family(symbol_family.clone())
                            .text_size(px(label_px))
                            .font_weight(FontWeight::NORMAL)
                            .text_color(if active { active_fg } else { muted })
                            .child(glyph),
                    )
                })
                .when(
                    !tree_mode && pane_count > 1 && !is_settings && logo_image.is_none() && !has_program_glyph,
                    |row| {
                        row.child(
                            div()
                                .w(px(icon_width))
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(pane_header::split_glyph(
                                    label_px * 0.78,
                                    if active { active_fg } else { muted },
                                )),
                        )
                    },
                )
                .when(tree_mode && !is_settings && logo_image.is_none() && !has_program_glyph && pane_count <= 1, |row| {
                    row.child(div().w(px(icon_width)).flex_shrink_0().child(Icon::new(IconName::SquareTerminal).small()))
                })
                // GPUI truncates the actual shaped title within its flex column.
                .child(match renaming {
                    Some(input) => div()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .flex()
                        .items_center()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                            if event.keystroke.key == "escape" {
                                cx.stop_propagation();
                                this.cancel_rename(window, cx);
                            }
                        }))
                        .child(
                            Input::new(&input)
                                .w_full()
                                .text_size(px(label_px))
                                .font_family(chrome_family.clone()),
                        )
                        .into_any_element(),
                    None => div()
                        .flex_1()
                        .min_w_0()
                        .font_family(chrome_family.clone())
                        .text_size(px(label_px))
                        .font_weight(FontWeight::NORMAL)
                        .truncate()
                        .child(title)
                        .into_any_element(),
                })
                .when(tree_mode && local_administrator, |row| {
                    row.child(tab_presentation::administrator_badge(format!("sidebar-admin-{ix}").into(), cx))
                })
                .when_some(tool, |row, tool| {
                    row.child(div().w(px(44.0)).h_full().flex_shrink_0().relative()
                        .child(h_flex().absolute().inset_0().items_center()
                            .when(tab_close_visible, |label| label.group_hover(hover_group.clone(), |style| style.invisible()))
                            .child(div().truncate().text_xs().text_color(theme.muted_foreground).child(tool)))
                        .when(tab_close_visible, |slot| slot.child(
                            h_flex().absolute().inset_0().items_center().justify_end()
                                .invisible().group_hover(hover_group.clone(), |style| style.visible())
                                .child(Button::new(("close-tab", ix)).ghost().xsmall().icon(IconName::Close)
                                    .aria_label(language.text(Message::CommonClose))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        cx.stop_propagation();
                                        this.request_close_tab(ix, window, cx);
                                    }))))))
                })
                .when(pane_count > 1 && !tree_mode, |row| {
                    row.child(
                        div()
                            .id(("sidebar-pane-count", ix))
                            .flex_shrink_0()
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.cycle_pane_focus(ix, window, cx);
                            }))
                            .child(pane_header::split_badge(
                                pane_count,
                                label_px,
                                if active { active_fg } else { muted },
                                if selected { active_bg } else { theme.muted },
                            )),
                    )
                })
                .child(
                    Self::tab_status_slot(status_width)
                        .when_some(resting_status, |slot, status| {
                            slot.child(
                                h_flex()
                                    .absolute()
                                    .inset_0()
                                    .justify_end()
                                    .items_center()
                                    .when(!session_row, |item| item.group_hover(hover_group.clone(), |item| item.invisible()))
                                    .child(status),
                            )
                        })
                        .when(!session_row, |slot| slot.child(
                            // 关闭按钮跟状态徽章共用同一个居中槽位：位置由
                            // flex 给出，不再硬写 top 偏移。
                            h_flex()
                                .absolute()
                                .inset_0()
                                .justify_end()
                                .items_center()
                                .invisible()
                                .group_hover(hover_group, |slot| slot.visible())
                                .when(tab_close_visible, |slot| {
                                        slot.child(
                                            Button::new(("close-tab", ix))
                                                .icon(IconName::Close)
                                                .ghost()
                                                .xsmall()
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        cx.stop_propagation();
                                                        this.request_close_tab(ix, window, cx);
                                                    },
                                                )),
                                        )
                                    }),
                        )),
                )
                // 右键只记锚点：菜单由 workspace 根上唯一一份宿主画。挂
                // `.context_menu()` 会让每个标签行都渲染同一个 PopupMenu，
                // 阴影按标签数叠厚（见 `tab_menu.rs` 模块头）。
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        this.open_tab_context_menu(ix, event.position, window, cx);
                    }),
                )
                .when_some(cross_window_drag, |row, payload| {
                    row.on_drag(payload, |payload, _, _, cx| {
                        NebulaWorkspace::cross_window_drag_preview(payload, cx)
                    })
                });
                let row = if dragged {
                    // 骑指针 + 提到最上层画（deferred 只延后绘制、不动布局），
                    // 阴影给"拿起来"的抬升感。
                    gpui::deferred(
                        row.top(px(drag.map(|(_, _, off)| off).unwrap_or(0.0))).shadow_md(),
                    )
                    .into_any_element()
                } else if shift != 0.0 {
                    // 让位滑动：进位方向 ease-out 滑入（旧壳是双向弹簧；回位
                    // 这里先直落，违和再补逐帧插值）。设置「标签动画=立即」时
                    // 直接落位（旧壳 TabRevealMotion::Instant 的 Snap 语义）。
                    if tab_reveal == nebula_settings::TabRevealName::Instant {
                        row.top(px(shift)).into_any_element()
                    } else {
                        row.with_animation(
                            ("tab-make-way", ix),
                            Animation::new(Duration::from_millis(120))
                                .with_easing(ease_out_quint()),
                            move |row, t| row.top(px(shift * t)),
                        )
                        .into_any_element()
                    }
                } else {
                    row.into_any_element()
                };
                if tree_mode { self.render_project_tab_branch(ix, row, Some(ix) == first_document, label_px, cx) } else { row }
            })
            .collect::<Vec<_>>();

        let header_group: SharedString = "sidebar-tabs-header-hover".into();
        let count: SharedString = indices.len().to_string().into();
        let language = crate::gpui_shell::config::ui_language(cx);

        if tree_mode {
            return self.render_project_session_sidebar(project_switcher, items, cx);
        }
        let tabs = self.render_tabs_section(items, cx);
        let sidebar = v_flex()
            .w(px(self.sidebar_width))
            .h_full()
            .flex_shrink_0()
            // workspace 根保持透明，侧栏自己只铺一层壳色；否则终端卡会
            // 叠到根底色上，把 Acrylic 的目标透明度二次增浓。
            .bg(theme.background)
            // 上边距为零：侧栏 / 终端卡 / 右侧抽屉三列顶边一律贴 chrome 下沿
            // （用户 08-26 裁定「左侧 tab 抬到和文件树顶部一致」）。左右和底部
            // 仍是旧壳的 8px。
            .px_2()
            .pb_2()
            .gap(px(tab_scroll::tab_row_gap(self.density)))
            // 待命阶段（未过阈值）的指针跟踪；激活后由根部罩层独占接管。
            .on_mouse_move(cx.listener(|this, event, window, cx| {
                this.update_tab_drag(event, window, cx);
            }))
            .child(project_switcher)
            .child(
                h_flex()
                    .id("sidebar-tabs-toggle")
                    .debug_selector(|| "sidebar-tabs-toggle".into())
                    .group(header_group.clone())
                    .w_full()
                    .h(px(row_height))
                    .pb_1()
                    // 旧壳标题文字从 panel_x + 16px 起；侧栏根已有 8px
                    // padding，这里再补 8px，箭头不会贴住左边缘。
                    .pl_2()
                    .items_center()
                    .cursor_pointer()
                    // `ChromeHit::TabsSection` 命中整条 tabs_header。监听必须
                    // 挂在标题行根节点，右侧空白区同样可以折叠，而不是只有
                    // 箭头、标题和数量这一小段能点。
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_tabs_section(cx);
                    }))
                    .child(
                        // 箭头、TABS 和计数仍作为一个排版段；折叠命中已经
                        // 提升到外层整行。右侧 +/⋯ 自己停止冒泡。
                        h_flex()
                            .h_full()
                            .items_center()
                            .font_family(chrome_family.clone())
                            .pr_1()
                            .child(
                                // 折叠三角用组件库线性 Chevron（lucide 细线），
                                // 不再用 Nerd Font 实心字位——后者在侧栏标题
                                // 上偏重、和右侧 +/⋯ 的 SVG 不一套语言。
                                h_flex()
                                    .w(px(TABS_DISCLOSURE_SLOT_W))
                                    .h_full()
                                    .flex_shrink_0()
                                    .items_center()
                                    .child(
                                        Icon::new(if self.tabs_section_collapsed {
                                            IconName::ChevronRight
                                        } else {
                                            IconName::ChevronDown
                                        })
                                        .xsmall()
                                        .text_color(muted),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(label_px * SIDEBAR_TITLE_SCALE))
                                    .font_weight(FontWeight::NORMAL)
                                    .text_color(muted)
                                    .child(language.text(Message::ProjectSessions)),
                            )
                            .child(
                                // chip 尺寸贴旧壳公式：h = max(cell_h×0.82×1.18, 11)
                                // ≈ 18-19px（不是 22），1 位数宽 max(adv+8, h×1.25)。
                                // 数字 ink_faint。
                                h_flex()
                                    .ml_2()
                                    .h(px((label_px * 1.22 * SIDEBAR_TITLE_SCALE * 1.18).max(11.0)))
                                    .min_w(px(label_px * 0.62 + 8.0))
                                    .px_2()
                                    .justify_center()
                                    .items_center()
                                    .rounded_full()
                                    .bg(theme.muted)
                                    .text_size(px(label_px * SIDEBAR_TITLE_SCALE))
                                    .font_weight(FontWeight::NORMAL)
                                    .text_color(faint)
                                    .child(count),
                            ),
                    )
                    .child(div().flex_1())
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .items_center()
                            .gap(px(2.0))
                            .child(
                                // 旧壳 `ChromeHit::NewTab`：直接开设置里的默认
                                // shell，不经过选择器。三点才是 NewTabMenu。
                                sidebar_new_tab_control(
                                    header_group,
                                    cx.listener(|this, _, window, cx| {
                                        cx.stop_propagation();
                                        this.add_terminal(window, cx);
                                    }),
                                )
                                    .rounded_md()
                                    .text_color(muted)
                                    .hover(|button| button.bg(hover_bg).text_color(theme.foreground))
                                    .tooltip(|window, cx| {
                                        gpui_component::tooltip::Tooltip::new(
                                            "新建终端 (Ctrl+Shift+T)",
                                        )
                                        .build(window, cx)
                                    })
                                    .child(
                                        Icon::new(IconName::Plus).with_size(px(SIDEBAR_HEADER_ICON)),
                                    ),
                            )
                            .child(
                                h_flex()
                                    .id("sidebar-tabs-menu")
                                    .w(px(SIDEBAR_MENU_W))
                                    .h(px(SIDEBAR_PLUS_SIZE))
                                    .flex_shrink_0()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .text_color(muted)
                                    .hover(|button| button.bg(hover_bg).text_color(theme.foreground))
                                    .tooltip(|window, cx| {
                                        gpui_component::tooltip::Tooltip::new("新建终端 (Ctrl+K)")
                                            .build(window, cx)
                                    })
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        cx.stop_propagation();
                                        this.open_shell_palette(window, cx);
                                    }))
                                    .child(
                                        Icon::new(IconName::EllipsisVertical)
                                            .with_size(px(SIDEBAR_HEADER_ICON)),
                                    ),
                            ),
                    ),
            )
            .child(tabs)
            .child(
                v_flex().w_full().flex_shrink_0().mt_auto().pt_1()
                    .child(self.render_usage_button(cx)),
            );
        sidebar.into_any_element()
    }

    /// 与旧壳 `nebula_tabs_section_open` 同义：点标题整行折叠/展开。
    /// 卷帘只裁剪槽位，视口高度在动画期间冻结，避免量到裁剪高后把溢出
    /// 列表锁成一行滚动区。
    fn toggle_tabs_section(&mut self, cx: &mut Context<Self>) {
        self.tabs_section_collapsed = !self.tabs_section_collapsed;
        // 卷帘一动，菜单锚定的那一行就不在原处了。
        self.tab_menu = None;
        self.tabs_fold_armed = true;
        self.tabs_fold_seq = self.tabs_fold_seq.wrapping_add(1).max(1);
        let seq = self.tabs_fold_seq;
        self.tabs_fold_frozen = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(250)).await;
            let _ = this.update(cx, |this, cx| {
                if this.tabs_fold_seq == seq {
                    this.tabs_fold_frozen = false;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// Tab 列表槽位。展开时列表是侧栏 `v_flex` 的 `flex_1` 子项，视口等于
    /// 面板剩余高度（旧壳 `tabs_avail`）。折叠动画只按**上次量到的剩余
    /// 高度**卷帘，绝不按全部行高，也不把裁剪高度写回窗口。
    fn render_tabs_section<I>(&self, items: I, cx: &mut Context<Self>) -> gpui::AnyElement
    where
        I: IntoIterator,
        I::Item: IntoElement,
    {
        let collapsed = self.tabs_section_collapsed;
        let list =
            self.wrap_tabs_scroll_list(items.into_iter().map(|item| item.into_any_element()), cx);
        if collapsed && !self.tabs_fold_frozen {
            return div().into_any_element();
        }
        if !self.tabs_fold_armed || !self.tabs_fold_frozen {
            return if collapsed { div().into_any_element() } else { list };
        }
        let slot_h = self.tabs_viewport_h;
        let (from, to) = if collapsed { (slot_h, 0.0) } else { (0.0, slot_h) };
        v_flex()
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_hidden()
            .child(list)
            .with_animation(
                ("tabs-fold", collapsed as usize),
                Animation::new(Duration::from_millis(240)).with_easing(ease_out_quint()),
                move |slot, t| {
                    let height = from + (to - from) * t;
                    if !collapsed && t >= 1.0 { slot } else { slot.max_h(px(height)) }
                },
            )
            .into_any_element()
    }

    /// 侧栏槽位：宽度在 0..持久化宽度间以 ease-out 滑动，近似旧壳
    /// response=0.14 的 swift-out 弹簧；内容保持固定宽、由槽位裁剪，
    /// 终端卡随 flex 布局自然滑移收编空间（对齐旧壳"卡骑在折叠动画上"
    /// 的观感）。动画按方向换 key 重启，端点随运行时设置变化。
    pub(super) fn render_sidebar_slot(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let collapsed = self.sidebar_collapsed;
        if tab_reveal_instant(cx) || !self.sidebar_fold_armed {
            return if collapsed {
                div().into_any_element()
            } else {
                self.render_sidebar(window, cx).into_any_element()
            };
        }
        let width = self.sidebar_closing_width.unwrap_or(self.sidebar_width);
        let (from, to) = if collapsed { (width, 0.0) } else { (0.0, width) };
        div()
            .h_full()
            .flex_shrink_0()
            .overflow_hidden()
            .child(self.render_sidebar(window, cx))
            .with_animation(
                ("sidebar-fold", collapsed as usize),
                Animation::new(Duration::from_millis(240)).with_easing(ease_out_quint()),
                move |slot, t| slot.w(px(from + (to - from) * t)),
            )
            .into_any_element()
    }

    /// 侧栏模式的标题栏：左边侧栏开关 + 齿轮，右边共享详情侧栏，中间在侧栏
    /// 折叠时顶上活动 tab 的名字。与顶部 tab 模式的 [`Self::render_top_title_bar`]
    /// 对称——两种布局各自持有自己那条标题带的全部内容。
    pub(super) fn render_sidebar_title_bar(
        &self,
        settings_active: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        // 颜色先取出来：`cx.theme()` 不可变借着 cx，后面每个 `cx.listener`
        // 都要可变借，混在一个表达式里借用检查过不去。
        let secondary = cx.theme().secondary;
        let settings_active_bg = cx.theme().sidebar_accent;
        let settings_active_fg = cx.theme().sidebar_accent_foreground;
        let sidebar_visible = !self.sidebar_collapsed && !self.reader_focus_active(cx);
        let language = crate::gpui_shell::config::ui_language(cx);
        let port_forward_button = self.render_port_forward_button(cx);
        h_flex()
            .size_full()
            .items_center()
            .justify_between()
            .child(
                h_flex()
                    // Keep toolbar gaps independent of the UI font/rem size.
                    .gap(px(8.0))
                    .items_center()
                    .occlude()
                    .child(
                        toolbar_button("toggle-sidebar", IconName::PanelLeft)
                            .disabled(settings_active)
                            // 侧栏是开关而非一次性动作：展开期间必须持续显示
                            // 选中底，和旧壳 `left_sidebar_visible()` 同义。
                            .selected(sidebar_visible)
                            // Ghost 的全局 selected 使用 hover_strong，静态底比
                            // 旧壳亮一档；仅此按钮覆写回旧壳 surface。
                            .when(sidebar_visible, |button| button.bg(secondary))
                            .tooltip(language.text(Message::ChromeToggleSidebar))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if this.reader_focus_active(cx) {
                                    this.clear_reader_focus(cx);
                                } else {
                                    this.sidebar_collapsed = !this.sidebar_collapsed;
                                }
                                this.sidebar_fold_armed = !tab_reveal_instant(cx);
                                cx.notify();
                            })),
                    )
                    .child(
                        toolbar_button("open-settings", IconName::Settings)
                            .selected(settings_active)
                            .when(settings_active, |button| {
                                button.bg(settings_active_bg).text_color(settings_active_fg)
                            })
                            .tooltip(language.text(Message::ChromeSettingsShortcut))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.toggle_settings(window, cx);
                            })),
                    ),
            )
            .child(self.render_collapsed_tab_title(cx))
            .child(
                title_bar_panel_controls()
                    .gap(px(8.0))
                    .child(self.render_terminal_find_button(settings_active, cx))
                    .child(
                        toolbar_button(
                            "toggle-command-manager",
                            Icon::new(Icon::empty())
                                .path(crate::gpui_shell::assets::nav::COMMAND_MANAGER),
                        )
                        .debug_selector(|| "command-manager-toggle".into())
                        .selected(self.command_manager_open)
                        .tooltip(language.text(Message::ChromeCommandList))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.toggle_command_manager(window, cx);
                        })),
                    )
                    .when_some(port_forward_button, |controls, button| controls.child(button))
                    .child(self.render_right_sidebar_button(settings_active, cx)),
            )
            .into_any_element()
    }

    /// 折叠态的活动 tab 名（旧壳 `chrome.rs` ~2129：侧栏收起后顶栏居中画活动
    /// tab 名，"没有侧栏也知道自己在哪"）。
    ///
    /// 弹性容器 + `absolute` 覆盖层的组合是刻意的：容器用 `flex_1` 抢下两侧
    /// 工具之间的空档，文字走覆盖层不参与布局——既不会把左右按钮挤歪，也不
    /// 吃鼠标事件，标题栏本身的拖窗和双击最大化照旧。
    fn render_collapsed_tab_title(&self, cx: &mut Context<Self>) -> gpui::Div {
        let slot = div().relative().flex_1().min_w_0().h_full();
        if self.settings_open {
            return slot.child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(cx.theme().foreground)
                    .child(crate::gpui_shell::config::ui_language(cx).pick("设置", "Settings")),
            );
        }
        if !self.sidebar_collapsed || self.tabs.is_empty() {
            return slot;
        }
        let theme = cx.theme();
        let (ink, dim, badge_fill) = (theme.foreground, theme.muted_foreground, theme.muted);
        let dark = theme.is_dark();
        let settings = cx.try_global::<crate::gpui_shell::config::Settings>();
        let chrome_family = theme.font_family.clone();
        let symbol_family: SharedString = crate::font_install::REQUIRED_FONT_FAMILY.into();
        let label_px = settings.map(|settings| settings.ui_font_size_px).unwrap_or(15.0);
        let TabPresentation {
            title,
            logo_image,
            logo_pending,
            program_glyph,
            pane_count,
            local_administrator,
            ..
        } = self.tab_presentation(self.active, cx, dark);
        slot.child(
            h_flex()
                .absolute()
                .inset_0()
                .items_center()
                .justify_center()
                .gap_2()
                // 两侧工具靠 flex 天然让位，这点内缩只是别让长标题贴到按钮上。
                .px_4()
                .when(local_administrator, |row| {
                    row.child(tab_presentation::administrator_badge(
                        "collapsed-title-admin".into(),
                        cx,
                    ))
                })
                .when_some(logo_image, |row, image| {
                    row.child(
                        img(image)
                            .size(px(TAB_LABEL_ICON_SIZE))
                            .flex_shrink_0()
                            .object_fit(ObjectFit::Contain),
                    )
                })
                .when_some(program_glyph, |row, glyph| {
                    row.child(
                        div()
                            .when(logo_pending, |slot| slot.w(px(TAB_LABEL_ICON_SIZE)))
                            .flex_shrink_0()
                            .font_family(symbol_family)
                            .text_size(px(label_px))
                            .text_color(dim)
                            .child(glyph),
                    )
                })
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .font_family(chrome_family)
                        .text_size(px(label_px))
                        .font_weight(FontWeight::NORMAL)
                        .text_color(ink)
                        .child(title),
                )
                // 折叠态没有侧栏行可看，分屏数量只能挂在这里；> 1 才画。
                .when(pane_count > 1, |row| {
                    row.child(pane_header::split_badge(pane_count, label_px, dim, badge_fill))
                }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "gpui-test-support")]
    mod layout {
        use super::*;
        use std::cell::RefCell;
        use std::rc::Rc;

        struct StatusProbe {
            label: SharedString,
            font_size: f32,
            mask: Rc<RefCell<Option<Bounds<Pixels>>>>,
        }

        impl Render for StatusProbe {
            fn render(
                &mut self,
                _window: &mut Window,
                _cx: &mut Context<Self>,
            ) -> impl IntoElement {
                let mask = self.mask.clone();
                h_flex()
                    .id("status-probe-row")
                    .debug_selector(|| "status-probe-row".to_owned())
                    .w(px(200.0))
                    .h(px(tab_scroll::tab_row_height(nebula_settings::DensityName::Standard)))
                    .px_2()
                    .overflow_hidden()
                    .child(div().flex_1())
                    .child(
                        NebulaWorkspace::tab_status_slot(TAB_STATUS_SLOT_W)
                            .id("status-probe-slot")
                            .debug_selector(|| "status-probe-slot".to_owned())
                            .child(
                                h_flex().absolute().inset_0().justify_end().items_center().child(
                                    NebulaWorkspace::shell_status_label(
                                        self.label.clone(),
                                        ".SystemUIFont".into(),
                                        self.font_size,
                                        gpui::black(),
                                    )
                                    .id("status-probe-label")
                                    .debug_selector(|| "status-probe-label".to_owned()),
                                ),
                            )
                            .child(
                                canvas(
                                    |_, _, _| {},
                                    move |_, _, window, _| {
                                        *mask.borrow_mut() = Some(window.content_mask().bounds);
                                    },
                                )
                                .absolute()
                                .size_full(),
                            ),
                    )
            }
        }

        #[gpui::test]
        fn short_shells_align_right_long_shells_fit_and_glyph_ink_keeps_row_padding(
            cx: &mut gpui::TestAppContext,
        ) {
            cx.update(gpui_component::init);
            for font_size in [12.0, 15.0, 20.0] {
                for label in ["sh", "debian", "a-very-long-shell-distribution-name"] {
                    let mask = Rc::new(RefCell::new(None));
                    let recorded = mask.clone();
                    let (_, visual) = cx.add_window_view(|_, _| StatusProbe {
                        label: label.into(),
                        font_size,
                        mask,
                    });
                    let row = visual.debug_bounds("status-probe-row").unwrap();
                    let slot = visual.debug_bounds("status-probe-slot").unwrap();
                    let text = visual.debug_bounds("status-probe-label").unwrap();
                    assert!(text.size.width <= slot.size.width);
                    assert_eq!(
                        text.right(),
                        slot.right(),
                        "shell labels retain the 1.7 right edge"
                    );
                    if label == "sh" {
                        assert!(text.size.width < slot.size.width, "short labels stay intrinsic");
                    }
                    let clip = recorded.borrow().expect("status paint mask");
                    assert!(clip.right() > slot.right(), "glyph overhang must retain row padding");
                    assert!(clip.right() <= row.right(), "the outer row still clips overflow");
                }
            }
        }
    }

    #[test]
    fn finished_dot_is_an_event_so_the_tab_you_are_watching_never_shows_it() {
        // 你正盯着 agent 跑完：那个点在你眼前亮起没有任何信息量。
        assert_eq!(resting_activity(SidebarActivity::Done, true, false), SidebarActivity::Idle);
        // 后台跑完才留痕。
        assert_eq!(resting_activity(SidebarActivity::Done, false, false), SidebarActivity::Done);
    }

    #[test]
    fn ongoing_states_show_on_the_active_tab_too() {
        // 状态描述「此刻仍然如此」，看不看都成立——当前 tab 一样要画。
        for state in [
            SidebarActivity::Running,
            SidebarActivity::Paused,
            SidebarActivity::WaitingInput,
            SidebarActivity::Attention,
            SidebarActivity::CommandFailed,
            SidebarActivity::Failed,
        ] {
            assert_eq!(resting_activity(state, true, false), state);
            assert_eq!(resting_activity(state, false, false), state);
            // 响铃补位只从 Idle 起跳，不能盖掉任何进行中的状态。
            assert_eq!(resting_activity(state, false, true), state);
        }
    }

    /// 对勾是「完成」的第一阶段，跟圆点同属事件：当前 tab 一律不画。命令失败
    /// 不是事件——看一眼不等于处理完了，所以它留在当前 tab 上。
    #[test]
    fn completion_flash_follows_the_same_unattended_rule_as_the_dot() {
        assert_eq!(
            resting_activity(SidebarActivity::Completed, true, false),
            SidebarActivity::Idle
        );
        assert_eq!(
            resting_activity(SidebarActivity::Completed, false, false),
            SidebarActivity::Completed
        );
        assert_eq!(
            resting_activity(SidebarActivity::CommandFailed, true, false),
            SidebarActivity::CommandFailed
        );
    }

    #[test]
    fn background_bell_stands_in_for_a_finish_mark_but_only_while_unattended() {
        assert_eq!(resting_activity(SidebarActivity::Idle, false, true), SidebarActivity::Done);
        assert_eq!(resting_activity(SidebarActivity::Idle, true, true), SidebarActivity::Idle);
        assert_eq!(resting_activity(SidebarActivity::Idle, false, false), SidebarActivity::Idle);
    }
}
