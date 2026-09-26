mod capture;
mod commands;
mod hotkeys;
mod overlay;
mod platform;
mod prices;
mod state;

use std::sync::Arc;
use tauri::{Manager, WindowEvent};

fn build_tray(app: &tauri::AppHandle, st: Arc<state::AppState>) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::TrayIconBuilder;
    let toggle = MenuItem::with_id(app, "toggle", "Afficher / masquer l'overlay", true, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Ouvrir la fenêtre principale", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quitter", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&toggle, &open, &quit])?;
    let mut tray = TrayIconBuilder::new().menu(&menu).tooltip("PoE2 Craft Assistant").on_menu_event(move |app, ev| match ev.id().as_ref() {
        "toggle" => overlay::toggle(app, &st),
        "open" => {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }
        "quit" => app.exit(0),
        _ => {}
    });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().with_handler(hotkeys::on_shortcut).build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
            let st = Arc::new(state::AppState::load(dir));
            app.manage(st.clone());
            let handle = app.handle().clone();
            overlay::init(&handle)?;
            // Un raccourci déjà pris ailleurs ne doit pas empêcher l'app de démarrer : l'erreur est visible dans les réglages.
            if let Err(e) = hotkeys::apply(&handle, &st) {
                eprintln!("raccourcis : {e}");
            }
            // L'icône de notification est un accès de secours indépendant du clavier : ne doit jamais empêcher le démarrage.
            if let Err(e) = build_tray(&handle, st.clone()) {
                eprintln!("icône de notification : {e}");
            }
            capture::spawn_watcher(handle.clone(), st.clone());
            prices::spawn_startup_refresh(handle.clone(), st.clone());
            overlay::spawn_follow(handle, st);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::dataset_info,
            commands::base_pool,
            commands::list_actions_cmd,
            commands::get_prices,
            commands::set_prices,
            commands::sandbox_apply,
            commands::item_detail,
            commands::run_simulation,
            commands::solve_plan,
            commands::cancel_job,
            commands::active_plan,
            commands::clear_active_plan,
            commands::submit_item_text,
            commands::analyze_item_text,
            commands::last_clipboard,
            commands::get_settings,
            commands::set_settings,
            commands::overlay_toggle,
            commands::overlay_set_interactive,
            commands::overlay_state,
            commands::check_update,
            commands::install_update,
            commands::app_version,
            commands::hotkey_status,
            commands::price_state,
            commands::refresh_prices,
        ])
        .on_window_event(|w, e| {
            // fermer la fenêtre principale quitte l'app (l'overlay caché ne doit pas la garder en vie)
            if let WindowEvent::CloseRequested { .. } = e {
                if w.label() == "main" {
                    w.app_handle().exit(0);
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("erreur au lancement de PoE2 Craft Assistant");
}
