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

/// Les deux raccourcis des réglages, lus et vérifiés (combinaisons valides et distinctes).
pub fn parse_pair(toggle: &str, interactive: &str) -> Result<(Shortcut, Shortcut), String> {
    let t: Shortcut = toggle.parse().map_err(|_| format!("raccourci invalide : « {toggle} »"))?;
    let i: Shortcut = interactive.parse().map_err(|_| format!("raccourci invalide : « {interactive} »"))?;
    if t == i {
        return Err("les deux raccourcis doivent être différents".into());
    }
    Ok((t, i))
}

fn apply_inner(app: &AppHandle, st: &AppState) -> Result<(), String> {
    let s = st.settings.lock().unwrap().clone();
    let (toggle, inter) = parse_pair(&s.hotkey_toggle, &s.hotkey_interactive)?;
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    // plus rien d'enregistré : un ancien raccourci ne doit jamais rester actif, même si l'enregistrement échoue
    *st.hotkeys.lock().unwrap() = None;
    gs.register(toggle).map_err(|e| format!("Le raccourci « {} » est indisponible (déjà utilisé par un autre logiciel ?) : {e}", s.hotkey_toggle))?;
    gs.register(inter).map_err(|e| format!("Le raccourci « {} » est indisponible (déjà utilisé par un autre logiciel ?) : {e}", s.hotkey_interactive))?;
    *st.hotkeys.lock().unwrap() = Some((toggle, inter));
    Ok(())
}

/// Suspend les raccourcis pendant leur saisie dans les réglages : une combinaison déjà enregistrée est captée par
/// Windows et n'arriverait jamais au champ de saisie.
pub fn suspend(app: &AppHandle, st: &AppState) {
    let _ = app.global_shortcut().unregister_all();
    *st.hotkeys.lock().unwrap() = None;
}

#[cfg(test)]
mod tests {
    use super::parse_pair;

    #[test]
    fn combinaisons_capturees_au_clavier_acceptees() {
        // format produit par src/lib/hotkey.ts (voir tools/hotkey.test.mjs)
        for (t, i) in [("Ctrl+S", "Ctrl+Shift+S"), ("F9", "Ctrl+F9"), ("Alt+Digit1", "Ctrl+NumpadAdd"), ("Ctrl+A", "Super+Shift+A")] {
            parse_pair(t, i).unwrap_or_else(|e| panic!("{t} / {i} : {e}"));
        }
    }

    #[test]
    fn ctrl_s_ne_repond_pas_a_ctrl_d() {
        let (t, i) = parse_pair("Ctrl+S", "Ctrl+Shift+S").unwrap();
        let (d, sd) = parse_pair("Ctrl+D", "Ctrl+Shift+D").unwrap();
        assert!(t != d && i != sd && t.id() != d.id());
    }

    #[test]
    fn combinaisons_refusees() {
        assert!(parse_pair("Ctrl+S", "ctrl+s").is_err());
        assert!(parse_pair("Ctrl+", "F9").is_err());
        assert!(parse_pair("Ctrl+S+D", "F9").is_err());
    }
}
