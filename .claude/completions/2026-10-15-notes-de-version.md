# Notes de version par release (0.2.48)

- `CHANGELOG.md` : section `## Prochaine version` remplie à chaque changement ; `tools/release.mjs` la renomme
  `## X.Y.Z (date)` et refuse de publier si elle est vide (`tools/changelog.mjs`, module sans dépendance Node).
- `build-windows.yml` : étape « Notes de version » (`tools/release-notes.mjs`) → `releaseBody` de tauri-action, qui sert
  aussi de champ `notes` de latest.json (vérifié dans le source de tauri-action v0).
- Appli : `src/components/ReleaseNotes.tsx` (fenêtre « Nouveautés » une fois par version, mémorisée en localStorage ;
  historique dans Réglages ; notes de la mise à jour proposée mises en forme). CHANGELOG embarqué via `?raw`.
- Tests : `tools/changelog.test.mjs` (npm test), dont la présence de notes pour la prochaine release.
- Pas de rattrapage des versions antérieures (choix de Max).
