//! Surveillance du presse-papiers (Ctrl+C / Ctrl+Alt+C en jeu) → analyse → conseil → événement vers l'UI.

use crate::state::AppState;
use craft_api::craft_data::looks_like_item;
use craft_api::{advise_item, analyze_item, AdviceResult, ItemAnalysis};
use serde::Serialize;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ItemCaptured {
    pub analysis: ItemAnalysis,
    pub advice: Option<AdviceResult>,
    pub advice_error: Option<String>,
    pub raw: String,
}

pub fn process_text(app: &AppHandle, st: &Arc<AppState>, text: &str) -> ItemCaptured {
    *st.last_clipboard.lock().unwrap() = text.to_string();
    let ds = st.dataset();
    let ilvl = st.settings.lock().unwrap().default_ilvl;
    let ctx = st.active.lock().unwrap().clone();
    let hint = ctx.as_ref().map(|c| c.req.base_id.clone());
    let analysis = analyze_item(&ds, text, hint.as_deref(), ilvl);

    let (mut advice, mut advice_error) = (None, None);
    if analysis.parsed.corrupted {
        // un objet corrompu ne peut plus être modifié : inutile de chercher une étape de craft
        advice_error = Some("Objet corrompu : aucune monnaie ne peut plus s'y appliquer.".to_string());
    } else if let (Some(ctx), Some(detail)) = (&ctx, &analysis.detail) {
        if analysis.base_id.as_deref() == Some(ctx.req.base_id.as_str()) {
            match detail.view.to_state(&ctx.bp.pool).and_then(|it| advise_item(ctx, &it, &AtomicBool::new(false))) {
                Ok(a) => advice = Some(a),
                Err(e) => advice_error = Some(e),
            }
        } else {
            advice_error = Some("cet objet n'est pas de la base du plan actif".into());
        }
    }
    let payload = ItemCaptured { analysis, advice, advice_error, raw: text.to_string() };
    let _ = app.emit("item-captured", &payload);
    if st.settings.lock().unwrap().auto_show_on_copy && payload.analysis.error.is_none() {
        crate::overlay::show_for_capture(app, st);
    }
    payload
}

fn read_clipboard() -> Option<String> {
    for _ in 0..4 {
        if let Ok(mut c) = arboard::Clipboard::new() {
            if let Ok(t) = c.get_text() {
                return Some(t);
            }
        }
        std::thread::sleep(Duration::from_millis(25)); // presse-papiers parfois verrouillé par le jeu
    }
    None
}

pub fn spawn_watcher(app: AppHandle, st: Arc<AppState>) {
    std::thread::spawn(move || {
        let (mut last_seq, mut last_hash) = (crate::platform::clipboard_seq(), 0u64);
        loop {
            std::thread::sleep(Duration::from_millis(120));
            if !st.settings.lock().unwrap().watch_clipboard {
                continue;
            }
            let seq = crate::platform::clipboard_seq();
            if seq != 0 && seq == last_seq {
                continue; // Windows : rien n'a changé, on ne lit même pas le contenu
            }
            last_seq = seq;
            let Some(text) = read_clipboard() else { continue };
            let mut h = DefaultHasher::new();
            text.hash(&mut h);
            let hash = h.finish();
            if seq == 0 && hash == last_hash {
                continue; // hors Windows : détection par empreinte
            }
            last_hash = hash;
            if looks_like_item(&text) {
                process_text(&app, &st, &text);
            }
        }
    });
}
