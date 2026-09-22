#!/usr/bin/env node
// Publie une version : met à jour les numéros, commit, tag vX.Y.Z, push. GitHub Actions construit et publie ensuite
// l'installeur signé + latest.json. Usage : node tools/release.mjs 0.2.0 [--dry-run]
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const [version, flag] = process.argv.slice(2);
const dry = flag === "--dry-run";
const fail = (m) => { console.error(`\nERREUR : ${m}\n`); process.exit(1); };
if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) fail("donne un numéro de version de la forme 0.2.0");

const at = (p) => resolve(root, p);
const json = (p, edit) => { const o = JSON.parse(readFileSync(at(p), "utf8")); edit(o); writeFileSync(at(p), JSON.stringify(o, null, 2) + "\n"); return o; };
const conf = JSON.parse(readFileSync(at("src-tauri/tauri.conf.json"), "utf8"));
const u = conf.plugins?.updater;
if (!u?.pubkey || String(u.endpoints).includes("OWNER/REPO")) {
  fail("les mises à jour ne sont pas configurées : lance d'abord setup-updates.bat (sinon les utilisateurs ne pourraient pas se mettre à jour).");
}
if (conf.version === version) fail(`la version ${version} est déjà celle du projet`);

const cargo = at("Cargo.toml");
const toml = readFileSync(cargo, "utf8");
if (!/\[workspace\.package\][^[]*?version = "[^"]+"/s.test(toml)) fail("version introuvable dans Cargo.toml");
if (!dry) {
  json("package.json", (o) => { o.version = version; });
  json("src-tauri/tauri.conf.json", (o) => { o.version = version; });
  writeFileSync(cargo, toml.replace(/(\[workspace\.package\][^[]*?version = ")[^"]+(")/s, `$1${version}$2`));
}
console.log(dry ? `(essai à blanc) numéros de version qui seraient écrits : ${version}` : `Numéros de version mis à jour : ${version}`);

const git = (...a) => { console.log(`> git ${a.join(" ")}`); if (!dry) execFileSync("git", a, { cwd: root, stdio: "inherit" }); };
git("add", "-A");
git("commit", "-m", `Version ${version}`);
git("tag", `v${version}`);
git("push", "origin", "HEAD", "--follow-tags");
console.log(dry
  ? "\n(essai à blanc : rien n'a été commité ni poussé)"
  : `\nPoussé. Suis la construction ici : onglet « Actions » de ton dépôt. Comptez ~15 minutes, puis la Release v${version} apparaît.`);
