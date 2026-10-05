# Dataset embarqué compressé (2026-10-05)

- `crates/craft-data/build.rs` compresse `data/sample/dataset.json` en gzip dans `OUT_DIR` ; `Dataset::embedded` le décompresse (flate2, déjà dans Cargo.lock via Tauri).
- Le fichier du dépôt reste du JSON minifié : `tools/import_repoe.mjs`, `update-dataset.bat` et les autres outils sont inchangés.
- Mesures (Linux, release) : JSON 1 133 106 o → 105 810 o embarqués ; `craft-cli` 4,33 Mo → 3,37 Mo ; chargement 4,1 → 6,1 ms (`cargo run --release -p craft-data --example load_time`).
- Test `embedded_gzip_matches_repository_json` : l'embarqué est identique octet pour octet au fichier.
