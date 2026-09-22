#!/usr/bin/env node
// Contrôle la Release publiée sur GitHub, comme le ferait l'application :
//   node tools/check-release.mjs [proprietaire/depot] [--url http://…/latest.json]
// Vérifie latest.json, la présence de l'installeur Windows, la signature et surtout qu'elle a été faite avec LA clé
// dont la partie publique est dans src-tauri/tauri.conf.json (sinon les applications refuseraient la mise à jour).
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const conf = JSON.parse(readFileSync(resolve(root, "src-tauri/tauri.conf.json"), "utf8"));
const args = process.argv.slice(2);
const urlArg = args.includes("--url") ? args[args.indexOf("--url") + 1] : null;
const repoArg = args.find((a) => /^[\w.-]+\/[\w.-]+$/.test(a));
const configured = conf.plugins?.updater ?? {};
let bad = 0;
const ok = (m) => console.log(`  ✓ ${m}`);
const ko = (m) => { bad++; console.log(`  ✗ ${m}`); };

const endpoint = urlArg ?? (repoArg ? `https://github.com/${repoArg}/releases/latest/download/latest.json` : configured.endpoints?.[0]);
console.log(`\nContrôle de la Release\n  adresse : ${endpoint}\n`);
if (!endpoint || endpoint.includes("OWNER/REPO")) { ko("mises à jour non configurées : lance setup-updates.bat"); process.exit(1); }

// « untrusted comment » + clé/signature minisign : octets 2..10 = identifiant de clé
const keyId = (b64) => {
  const text = Buffer.from(b64.trim(), "base64").toString("utf8");
  const line = text.split(/\r?\n/).filter(Boolean)[1];
  return line ? Buffer.from(line, "base64").subarray(2, 10).toString("hex") : null;
};

let latest;
try {
  const r = await fetch(endpoint, { redirect: "follow" });
  if (!r.ok) throw new Error(`HTTP ${r.status}${r.status === 404 ? " (Release absente, ou dépôt privé : il doit être public)" : ""}`);
  latest = await r.json();
  ok("latest.json téléchargé");
} catch (e) { ko(`latest.json inaccessible : ${e.message}`); process.exit(1); }

/^\d+\.\d+\.\d+/.test(latest.version ?? "") ? ok(`version publiée : ${latest.version}`) : ko(`version invalide : ${latest.version}`);
if (latest.version === conf.version) ok(`identique à celle du dossier local (${conf.version})`);
else console.log(`  · dossier local en ${conf.version}, Release en ${latest.version}`);

const entry = Object.entries(latest.platforms ?? {}).find(([k]) => k.startsWith("windows-x86_64"));
if (!entry) { ko(`aucune plateforme windows-x86_64 (trouvé : ${Object.keys(latest.platforms ?? {}).join(", ") || "rien"})`); process.exit(1); }
const [platform, p] = entry;
ok(`plateforme ${platform}`);
/\.exe(\?|$)/i.test(p.url ?? "") ? ok(`installeur : ${decodeURIComponent(p.url.split("/").pop())}`) : ko(`l'URL ne pointe pas vers un .exe : ${p.url}`);

try {
  const h = await fetch(p.url, { method: "HEAD", redirect: "follow" });
  h.ok ? ok(`l'installeur répond (${h.status}${h.headers.get("content-length") ? `, ${(h.headers.get("content-length") / 1e6).toFixed(1)} Mo` : ""})`) : ko(`l'installeur répond HTTP ${h.status}`);
} catch (e) { ko(`installeur injoignable : ${e.message}`); }

if (!p.signature) ko("signature absente : le build n'a pas signé (secrets TAURI_SIGNING_PRIVATE_KEY* manquants ou createUpdaterArtifacts désactivé)");
else if (!configured.pubkey) ko("aucune clé publique dans tauri.conf.json");
else {
  const [a, b] = [keyId(p.signature), keyId(configured.pubkey)];
  if (!a || !b) ko("signature ou clé publique illisibles (format inattendu)");
  else if (a === b) ok(`signature faite avec la bonne clé (identifiant ${a})`);
  else ko(`signature faite avec une AUTRE clé (${a}) que celle de l'application (${b}) : les apps refuseront cette mise à jour. Le secret GitHub ne correspond pas à .tauri/poe2craft.key`);
}
console.log(bad ? `\n${bad} problème(s) : cette Release ne permettra pas la mise à jour.\n` : "\nTout est bon : les applications installées avec cette clé accepteront cette version.\n");
process.exit(bad ? 1 : 0);
