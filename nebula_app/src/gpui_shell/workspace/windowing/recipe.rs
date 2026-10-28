//! A recipe is opened once per process; activation never replays its startup commands.

use super::super::recipes::Recipe;
use super::*;

pub(in crate::gpui_shell::workspace) fn open_recipe_window(
    recipe: &Recipe,
    cx: &mut App,
) -> Result<(), String> {
    prune_entries(cx);
    if let Some(entry) = cx
        .global::<WindowRegistry>()
        .entries
        .iter()
        .find(|entry| {
            recipe.startup.is_some()
                && entry.recipe_source.as_ref() == Some(&recipe.path)
                && entry.workspace.upgrade().is_some_and(|workspace| {
                    workspace.read(cx).tabs.iter().any(WorkspaceTab::is_terminal)
                })
        })
        .cloned()
    {
        focus_entry(&entry, None, cx);
        return Ok(());
    }
    let (id, _) =
        open_workspace_window(cx, WorkspaceStartup::Empty, None, None, true, WindowRole::Regular)
            .map_err(|error| error.to_string())?;
    let entry = cx
        .global::<WindowRegistry>()
        .entries
        .iter()
        .find(|entry| entry.runtime_window_id == id)
        .cloned()
        .ok_or("Recipe window is unavailable")?;
    entry
        .handle
        .update(cx, |_, window, cx| {
            entry
                .workspace
                .update(cx, |workspace, cx| {
                    for tab in &recipe.session.tabs {
                        if !workspace.restore_tab(tab, false, window, cx) {
                            return Err("Could not restore the recipe layout".to_owned());
                        }
                        if let Some(root) = &tab.project_root {
                            workspace.queue_project_change(
                                crate::project_library::Change::Open(root.clone()),
                                cx,
                            );
                        }
                    }
                    if let Some(starts) = &recipe.startup {
                        for start in starts {
                            let Some(WorkspaceTab::Terminal { panes, .. }) =
                                workspace.tabs.get(start.tab)
                            else {
                                return Err("Project startup tab is unavailable".into());
                            };
                            let pane = panes
                                .get(start.pane)
                                .ok_or("Project startup pane is unavailable")?;
                            let command =
                                start.command().ok_or("Unsupported project startup tool")?;
                            pane.view.update(cx, |view, cx| view.run_command(command, cx));
                        }
                    }
                    workspace.active =
                        recipe.session.active_tab.min(workspace.tabs.len().saturating_sub(1));
                    workspace.reveal_active_project();
                    workspace.focus_active(window, cx);
                    cx.notify();
                    Ok::<_, String>(())
                })
                .map_err(|error| error.to_string())?
        })
        .map_err(|error| error.to_string())??;
    if let Some(entry) = cx
        .global_mut::<WindowRegistry>()
        .entries
        .iter_mut()
        .find(|entry| entry.runtime_window_id == id)
    {
        entry.recipe_source = recipe.startup.as_ref().map(|_| recipe.path.clone());
    }
    Ok(())
}
