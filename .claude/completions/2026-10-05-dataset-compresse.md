# 2026-10-05 : dataset embarqué compressé

- `crates/craft-data/build.rs` compresse `data/sample/dataset.json` (1 133 109 octets) en gzip niveau 9
  (~105 Ko) dans OUT_DIR ; `dataset.rs` l'embarque par `include_bytes!` et le décompresse une seule fois
  (`OnceLock`) avec flate2 (moteur Rust pur miniz_oxide, déjà dans l'arbre de Tauri).
- Mesuré (craft-cli release, Linux) : 4 330 448 → 3 371 096 octets (−959 Ko) ; démarrage `craft-cli bases`
  7,5 → 10,3 ms (médiane de 15) ; sortie identique.
- Rien ne change pour les outils (`tools/*.mjs`, update-dataset.bat) : le fichier du dépôt reste du JSON.
- Chargement par parties écarté : le front ne reçoit jamais le dataset entier (commandes Tauri ciblées), et
  tout le contenu sert à construire les pools ; le seul gain serait le parse (~3 ms).
- Test : `embedded_dataset_is_compressed_copy_of_repo_file` (version embarquée = fichier du dépôt, octet pour octet).
- Routines créées : vérif hebdo RePoE (lundi 7h52 Paris), vérif dataset après release (chaque jour 8h47).
