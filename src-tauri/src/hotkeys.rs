use crate::state::AppState;
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState};

pub fn on_shortcut(app: &AppHandle, sc: &Shortcut, ev: ShortcutEvent) {
    if ev.state() != ShortcutState::Pressed {
        return;
    }
    let Some(st) = app.try_state::<Arc<AppState>>() else { return };
    let hk = *st.hotkeys.lock().unwrap();
    if let Some((toggle, interactive)) = hk {
        if *sc == toggle {
            crate::overlay::toggle(app, &st);
        } else if *sc == interactive {
            crate::overlay::toggle_interactive(app, &st);
        }
    }
}

/// (Ré)enregistre les raccourcis depuis les réglages. Renvoie une erreur lisible si une combinaison est invalide
/// ou déjà prise par une autre application.
pub fn apply(app: &AppHandle, st: &AppState) -> Result<(), String> {
    let r = apply_inner(app, st);
    *st.hotkey_error.lock().unwrap() = r.as_ref().err().cloned();
    r
}

fn apply_inner(app: &AppHandle, st: &AppState) -> Result<(), String> {
    let s = st.settings.lock().unwrap().clone();
    let toggle: Shortcut = s.hotkey_toggle.parse().map_err(|_| format!("raccourci invalide : « {} »", s.hotkey_toggle))?;
    let inter: Shortcut = s.hotkey_interactive.parse().map_err(|_| format!("raccourci invalide : « {} »", s.hotkey_interactive))?;
    if toggle == inter {
        return Err("les deux raccourcis doivent être différents".into());
    }
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    gs.register(toggle).map_err(|e| format!("Le raccourci « {} » est indisponible (déjà utilisé par un autre logiciel ?) : {e}", s.hotkey_toggle))?;
    gs.register(inter).map_err(|e| format!("Le raccourci « {} » est indisponible (déjà utilisé par un autre logiciel ?) : {e}", s.hotkey_interactive))?;
    *st.hotkeys.lock().unwrap() = Some((toggle, inter));
    Ok(())
}
