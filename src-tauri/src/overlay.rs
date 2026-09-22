use crate::platform;
use crate::state::AppState;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize};

const LABEL: &str = "overlay";

/// Poignée Win32 de la fenêtre (0 hors Windows : `WebviewWindow::hwnd` n'existe que sur cette plateforme).
fn hwnd_of(w: &tauri::WebviewWindow) -> isize {
    #[cfg(windows)]
    {
        w.hwnd().map(|h| h.0 as isize).unwrap_or(0)
    }
    #[cfg(not(windows))]
    {
        let _ = w;
        0
    }
}

pub fn init(app: &AppHandle) -> Result<(), String> {
    let w = app.get_webview_window(LABEL).ok_or("fenêtre overlay introuvable")?;
    // Le clic-traversant est (ré)appliqué à chaque affichage (voir `refresh`) : sous Linux, la fenêtre doit exister
    // à l'écran pour l'accepter.
    #[cfg(windows)]
    w.set_ignore_cursor_events(true).map_err(|e| e.to_string())?;
    let h = hwnd_of(&w);
    if h != 0 {
        platform::apply_overlay_styles(h);
    }
    Ok(())
}

fn overlay_hwnd(app: &AppHandle) -> isize {
    app.get_webview_window(LABEL).map(|w| hwnd_of(&w)).unwrap_or(0)
}

/// Place le panneau dans le coin supérieur droit de la zone cliente du jeu (ou de l'écran principal sans jeu).
fn place(app: &AppHandle, st: &AppState, game: Option<platform::GameWindow>) {
    let Some(w) = app.get_webview_window(LABEL) else { return };
    let s = st.settings.lock().unwrap().clone();
    let scale = w.scale_factor().unwrap_or(1.0);
    let (pw, ph) = ((s.overlay_width as f64 * scale) as i32, (s.overlay_height as f64 * scale) as i32);
    let (mx, my) = ((s.overlay_margin_x as f64 * scale) as i32, (s.overlay_margin_y as f64 * scale) as i32);
    let (right, top) = match game {
        Some(g) => (g.right, g.top),
        None => match app.primary_monitor().ok().flatten() {
            Some(m) => (m.position().x + m.size().width as i32, m.position().y),
            None => return,
        },
    };
    let _ = w.set_size(PhysicalSize::new(pw.max(200) as u32, ph.max(200) as u32));
    let _ = w.set_position(PhysicalPosition::new(right - pw - mx, top + my));
}

fn should_show(app: &AppHandle, st: &AppState, game: Option<platform::GameWindow>) -> bool {
    if !st.overlay_wanted.load(Ordering::Relaxed) {
        return false;
    }
    match game {
        // le jeu est lancé : on n'affiche l'overlay que si le jeu (ou l'overlay lui-même) a le focus
        Some(g) => {
            let fg = platform::foreground();
            fg == g.hwnd || fg == overlay_hwnd(app)
        }
        // pas de jeu : affichage manuel (test de l'app avant de lancer PoE2)
        None => true,
    }
}

pub fn refresh(app: &AppHandle, st: &AppState) {
    let Some(w) = app.get_webview_window(LABEL) else { return };
    let title = st.settings.lock().unwrap().game_window_title.clone();
    let game = platform::find_game(&title);
    let show = should_show(app, st, game);
    if show {
        place(app, st, game);
        if !w.is_visible().unwrap_or(false) {
            let _ = w.show();
            let _ = w.set_ignore_cursor_events(!st.overlay_interactive.load(Ordering::Relaxed));
        }
    } else if w.is_visible().unwrap_or(false) {
        let _ = w.hide();
    }
}

pub fn set_wanted(app: &AppHandle, st: &AppState, v: bool) {
    st.overlay_wanted.store(v, Ordering::Relaxed);
    let _ = app.emit("overlay-wanted", v);
    refresh(app, st);
}

/// Bascule manuelle (raccourci, icône de notification, bouton de l'app) : l'overlay reste jusqu'à la prochaine bascule.
pub fn toggle(app: &AppHandle, st: &AppState) {
    st.auto_shown.store(false, Ordering::Relaxed);
    set_wanted(app, st, !st.overlay_wanted.load(Ordering::Relaxed));
}

/// Affichage déclenché par une copie d'objet : disparaît seul après `overlay_auto_hide_secs`.
/// Si l'overlay était déjà ouvert à la main, on n'y touche pas.
pub fn show_for_capture(app: &AppHandle, st: &AppState) {
    *st.last_capture.lock().unwrap() = std::time::Instant::now();
    if !st.overlay_wanted.load(Ordering::Relaxed) {
        st.auto_shown.store(true, Ordering::Relaxed);
        set_wanted(app, st, true);
    }
}

pub fn set_interactive(app: &AppHandle, st: &AppState, v: bool) {
    if v {
        st.auto_shown.store(false, Ordering::Relaxed); // l'utilisateur a pris la main : plus de masquage automatique
    }
    st.overlay_interactive.store(v, Ordering::Relaxed);
    if let Some(w) = app.get_webview_window(LABEL) {
        // fenêtre cachée : le réglage sera appliqué à son prochain affichage
        if w.is_visible().unwrap_or(false) {
            let _ = w.set_ignore_cursor_events(!v);
        }
    }
    let _ = app.emit("overlay-interactive", v);
}

pub fn toggle_interactive(app: &AppHandle, st: &AppState) {
    set_interactive(app, st, !st.overlay_interactive.load(Ordering::Relaxed));
}

/// Suit la fenêtre du jeu : position, focus, minimisation.
pub fn spawn_follow(app: AppHandle, st: Arc<AppState>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(250));
        let secs = st.settings.lock().unwrap().overlay_auto_hide_secs as u64;
        if secs > 0
            && st.auto_shown.load(Ordering::Relaxed)
            && st.overlay_wanted.load(Ordering::Relaxed)
            && !st.overlay_interactive.load(Ordering::Relaxed)
            && st.last_capture.lock().unwrap().elapsed().as_secs() >= secs
        {
            st.auto_shown.store(false, Ordering::Relaxed);
            set_wanted(&app, &st, false);
        }
        refresh(&app, &st);
    });
}
