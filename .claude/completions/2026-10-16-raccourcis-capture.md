# 2026-10-16 — Raccourcis : Ctrl+D fantôme, capture au clavier, historique limité

- Cause probable du Ctrl+D qui affichait l'overlay malgré Ctrl+S configuré : deuxième instance de l'appli (aucune protection single-instance) gardant Ctrl+D et son propre overlay, ou réglage tapé sans cliquer « Enregistrer » (bouton en bas de page). Le code d'enregistrement (`unregister_all` puis `register`) était correct.
- Correctifs : `tauri-plugin-single-instance` (relancer ramène la fenêtre), commande `set_hotkeys` appliquée et enregistrée dès la saisie, `hotkeys_pause` pendant la capture (sinon Windows avale la combinaison déjà enregistrée), `st.hotkeys` vidé avant tout ré-enregistrement.
- Capture au clavier : `src/lib/hotkey.ts` (lettres selon la disposition, chiffres/touches F selon la position), tests `tools/hotkey.test.mjs` + `hotkeys::tests` (Rust).
- Historique des versions limité aux 5 dernières (`ReleaseNotes.tsx`).
