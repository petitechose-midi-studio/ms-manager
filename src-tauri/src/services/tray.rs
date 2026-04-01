use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::{App, Manager, Runtime};

use crate::{services, state};

pub fn install<R: Runtime>(app: &mut App<R>) -> tauri::Result<()> {
    let show = MenuItemBuilder::new("Open ms-manager")
        .id("show")
        .build(app)?;
    let quit = MenuItemBuilder::new("Quit").id("quit").build(app)?;
    let menu = MenuBuilder::new(app).items(&[&show, &quit]).build()?;
    TrayIconBuilder::new()
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let bindings = app.state::<state::AppState>().bridge_instances_get();
                    for binding in bindings.instances.iter().filter(|binding| binding.enabled) {
                        let _ = services::bridge_ctl::send_command(
                            binding.control_port,
                            "shutdown",
                            std::time::Duration::from_millis(700),
                        )
                        .await;
                    }

                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;

                    let layout = app.state::<state::AppState>().layout_get();
                    let exe =
                        services::artifact_resolver::resolve_management_oc_bridge_exe(&layout)
                            .unwrap_or_else(|_| services::payload::oc_bridge_path(&layout));
                    let _ = services::bridge_process::kill_oc_bridge_daemons(&exe);
                    let _ = services::bridge_process::kill_all_oc_bridge_daemons();

                    app.exit(0);
                });
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::DoubleClick { .. } = event {
                if let Some(window) = tray.app_handle().get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}
