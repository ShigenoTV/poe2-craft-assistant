// Notes de version : CHANGELOG.md à la racine, une section « ## X.Y.Z (AAAA-MM-JJ) » par version publiée,
// plus une section « ## Prochaine version » où s'accumulent les notes en attente de publication.
// release.mjs renomme cette section avec le numéro publié ; le workflow en fait le texte de la Release
// et le champ « notes » de latest.json ; l'appli embarque le fichier (fenêtre Nouveautés, historique).
// Module sans dépendance Node : il est aussi embarqué dans l'appli (src/lib/changelog.ts).

export const NEXT = "Prochaine version";

/** Sections du fichier, dans l'ordre du fichier (la plus récente en haut). */
export function parseChangelog(text) {
  const out = [];
  let cur = null;
  for (const line of text.replace(/\r\n?/g, "\n").split("\n")) {
    const h = /^## +(.+?)\s*$/.exec(line);
    if (h) {
      const m = /^(\d+\.\d+\.\d+)(?:\s*\((\d{4}-\d{2}-\d{2})\))?$/.exec(h[1]);
      cur = { version: m ? m[1] : h[1], date: m?.[2] ?? null, body: [] };
      out.push(cur);
    } else if (cur) cur.body.push(line);
  }
  return out.map((s) => ({ version: s.version, date: s.date, body: s.body.join("\n").trim() }));
}

/** Vrai si le texte dit quelque chose (des titres « ### » seuls ne comptent pas). */
const hasContent = (body) => body.split("\n").some((l) => l.trim() && !l.trim().startsWith("#"));

/** Notes d'une version (`null` si absente ou vide). */
export function notesFor(text, version) {
  return parseChangelog(text).find((s) => s.version === version && hasContent(s.body))?.body ?? null;
}

/** Renomme « ## Prochaine version » en « ## X.Y.Z (date) » ; erreur si la section manque ou est vide. */
export function stampNext(text, version, date) {
  if (!notesFor(text, NEXT)) throw new Error(`aucune note sous « ## ${NEXT} » dans CHANGELOG.md : écris d'abord ce que contient la ${version}`);
  if (parseChangelog(text).some((s) => s.version === version)) throw new Error(`CHANGELOG.md contient déjà une section ${version}`);
  return text.replace(new RegExp(`^## +${NEXT}\\s*$`, "m"), `## ${version} (${date})`);
}

/** Versions publiées strictement plus récentes que `since` et jusqu'à `upTo` inclus. */
export function newerThan(text, since, upTo) {
  const cmp = (a, b) => { const x = a.split(".").map(Number), y = b.split(".").map(Number); for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return x[i] - y[i]; return 0; };
  const isV = (v) => /^\d+\.\d+\.\d+$/.test(v);
  return parseChangelog(text).filter((s) => isV(s.version) && hasContent(s.body) && (!since || !isV(since) || cmp(s.version, since) > 0) && cmp(s.version, upTo) <= 0);
}

