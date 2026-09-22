#!/usr/bin/env node
// Configure les mises à jour automatiques (une seule fois) :
//   1. crée une paire de clés de signature dans .tauri/ (jamais commitée),
//   2. inscrit la clé PUBLIQUE et l'adresse de ton dépôt GitHub dans src-tauri/tauri.conf.json,
//   3. t'indique les 2 secrets à ajouter sur GitHub.
// Usage : node tools/setup-updates.mjs [proprietaire/depot]
import { execFileSync, spawnSync } from "node:child_process";
import { randomBytes } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const confPath = resolve(root, "src-tauri/tauri.conf.json");
const keyDir = resolve(root, ".tauri");
const keyPath = resolve(keyDir, "poe2craft.key");
const passPath = resolve(keyDir, "password.txt");
const fail = (m) => { console.error(`\nERREUR : ${m}\n`); process.exit(1); };

function detectRepo() {
  const arg = process.argv[2];
  if (arg) return /^[\w.-]+\/[\w.-]+$/.test(arg) ? arg : fail(`« ${arg} » n'a pas la forme proprietaire/depot`);
  let url = "";
  try { url = execFileSync("git", ["remote", "get-url", "origin"], { cwd: root, stdio: ["ignore", "pipe", "ignore"] }).toString().trim(); } catch { /* pas de remote */ }
  const m = url.match(/github\.com[:/]([\w.-]+)\/([\w.-]+?)(?:\.git)?$/);
  return m ? `${m[1]}/${m[2]}` : fail("dépôt GitHub introuvable. Ajoute-le d'abord (git remote add origin https://github.com/TOI/DEPOT.git) ou lance : node tools/setup-updates.mjs TOI/DEPOT");
}

const repo = detectRepo();

if (!existsSync(keyPath)) {
  mkdirSync(keyDir, { recursive: true });
  const password = randomBytes(16).toString("hex");
  const r = spawnSync("npx", ["tauri", "signer", "generate", "--ci", "-p", password, "-w", keyPath], { cwd: root, stdio: "inherit", shell: true });
  if (r.status !== 0 || !existsSync(`${keyPath}.pub`)) fail("la génération de la clé a échoué (npm ci a-t-il été lancé ?)");
  writeFileSync(passPath, password); // sans retour à la ligne : un espace parasite dans le secret casse la signature en CI
  console.log("\nNouvelle paire de clés créée dans .tauri/");
} else {
  console.log("Clés déjà présentes dans .tauri/ : réutilisées (jamais écrasées).");
}

const conf = JSON.parse(readFileSync(confPath, "utf8"));
conf.plugins ??= {};
conf.plugins.updater = {
  ...(conf.plugins.updater ?? {}),
  pubkey: readFileSync(`${keyPath}.pub`, "utf8").trim(),
  endpoints: [`https://github.com/${repo}/releases/latest/download/latest.json`],
};
writeFileSync(confPath, JSON.stringify(conf, null, 2) + "\n");

// Ajout automatique des secrets si la CLI GitHub (gh) est installée et connectée ; valeurs nettoyées de tout espace.
function trySetSecrets() {
  if (spawnSync("gh", ["--version"], { stdio: "ignore" }).status !== 0) return false;
  return [["TAURI_SIGNING_PRIVATE_KEY", keyPath], ["TAURI_SIGNING_PRIVATE_KEY_PASSWORD", passPath]].every(([name, file]) =>
    spawnSync("gh", ["secret", "set", name, "--repo", repo], { input: readFileSync(file, "utf8").trim(), stdio: ["pipe", "ignore", "pipe"] }).status === 0);
}
const secretsDone = trySetSecrets();
console.log(secretsDone
  ? `\nSecrets GitHub ajoutés automatiquement pour ${repo} (via gh).`
  : "\n(gh absent, non connecté, ou dépôt pas encore poussé : ajoute les secrets à la main, voir ci-dessous.)");

console.log(`
Configuration écrite dans src-tauri/tauri.conf.json  (dépôt : ${repo})

À FAIRE UNE FOIS sur GitHub  -->  https://github.com/${repo}/settings/secrets/actions
  Bouton « New repository secret », deux fois :
    Nom : TAURI_SIGNING_PRIVATE_KEY            Valeur : tout le contenu du fichier  ${keyPath}
                                               (sélectionne le texte seulement, sans ligne vide avant ou après)
    Nom : TAURI_SIGNING_PRIVATE_KEY_PASSWORD   Valeur : le contenu du fichier       ${passPath}

  (avec la CLI GitHub installée, en une commande :
     gh secret set TAURI_SIGNING_PRIVATE_KEY < .tauri/poe2craft.key
     gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD < .tauri/password.txt )

SAUVEGARDE le dossier .tauri/ ailleurs (il est ignoré par git). Sans cette clé, les installations existantes
ne pourront plus jamais se mettre à jour. Ne la partage avec personne.

Ensuite : commit + push de tauri.conf.json, puis publie une version avec  release.bat
`);
