#!/usr/bin/env node
// Affiche les notes de CHANGELOG.md pour une version (utilisé par build-windows.yml pour le texte de la Release
// et le champ « notes » de latest.json). Usage : node tools/release-notes.mjs X.Y.Z  (pas de notes → code 1)
import { readFileSync } from "node:fs";
import { notesFor } from "./changelog.mjs";

const version = process.argv[2];
if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) { console.error("usage : node tools/release-notes.mjs X.Y.Z"); process.exit(2); }
const notes = notesFor(readFileSync(new URL("../CHANGELOG.md", import.meta.url), "utf8"), version);
if (!notes) { console.error(`pas de notes pour ${version} dans CHANGELOG.md`); process.exit(1); }
process.stdout.write(notes + "\n");
