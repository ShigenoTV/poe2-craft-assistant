import { useEffect, useState } from "react";
import { api, listen } from "@/lib/ipc";
import { useStore } from "@/store";
import type { PriceState, Settings } from "@/lib/types";
import { num, shortUnit } from "@/lib/format";

function UpdatesPanel({ s, set }: { s: Settings; set: <K extends keyof Settings>(k: K, v: Settings[K]) => void }) {
  const { version, update, updateStatus, updateProgress, updateError, checkUpdate, installUpdate } = useStore();
  useEffect(() => { if (!version) void api.appVersion().then((v) => useStore.setState({ version: v })); }, [version]);
  const busy = updateStatus === "checking" || updateStatus === "installing";
  return (
    <div className="panel pad stack" style={{ gap: 10 }}>
      <div className="row"><h3 className="hd">Mises à jour</h3><span className="muted small">version installée {version || "…"}</span></div>
      <div className="row">
        <button className="btn" disabled={busy} onClick={() => void checkUpdate(true)}>{updateStatus === "checking" ? "Recherche…" : "Rechercher une mise à jour"}</button>
        {update && updateStatus !== "installing" && <button className="btn primary" onClick={() => void installUpdate()}>Installer la version {update.version} et redémarrer</button>}
      </div>
      {updateStatus === "uptodate" && <div className="note ok-t">Tu utilises la dernière version.</div>}
      {updateStatus === "available" && update && (
        <div className="note">Version {update.version} disponible{update.date ? ` (${update.date.slice(0, 10)})` : ""}.{update.notes && <><br />{update.notes}</>}</div>
      )}
      {updateStatus === "installing" && (
        <div><div className={`progress ${updateProgress?.total ? "" : "ind"}`}><i style={{ width: `${updateProgress?.total ? (100 * updateProgress.downloaded) / updateProgress.total : 0}%` }} /></div>
          <div className="small muted" style={{ marginTop: 4 }}>{updateProgress?.total ? `Téléchargement : ${num(updateProgress.downloaded / 1e6, 1)} / ${num(updateProgress.total / 1e6, 1)} Mo` : "Installation… l'application va redémarrer."}</div></div>
      )}
      {updateStatus === "unconfigured" && <div className="note">Les mises à jour ne sont pas encore configurées pour cette copie : lance <span className="kbd">setup-updates.bat</span> une fois (voir le README).</div>}
      {updateStatus === "error" && <div className="err">{updateError}</div>}
      <label className="row"><input type="checkbox" checked={s.checkUpdatesOnStart} onChange={(e) => set("checkUpdatesOnStart", e.target.checked)} /> Rechercher une mise à jour au démarrage (le choix s'applique après « Enregistrer »)</label>
      <p className="note small">Rien n'est installé sans ton accord. Chaque mise à jour est vérifiée par signature avant installation.</p>
    </div>
  );
}

const ago = (secs: number) => (secs < 90 ? "à l'instant" : secs < 5400 ? `il y a ${Math.round(secs / 60)} min` : `il y a ${Math.round(secs / 3600)} h`);

function PricesEditor({ s, set }: { s: Settings; set: <K extends keyof Settings>(k: K, v: Settings[K]) => void }) {
  const { reloadPrices, info, actions } = useStore();
  const [ps, setPs] = useState<PriceState | null>(null);
  const [edit, setEdit] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const unit = shortUnit(info?.priceUnit);

  useEffect(() => {
    void api.priceState().then(setPs);
    let off = () => {};
    void listen<PriceState>("prices-updated", setPs).then((f) => (off = f));
    return () => off();
  }, []);

  const label = (k: string) =>
    k === "base_white" ? "Base neuve (objet blanc)" : k === "base_salvage" ? "Revente d'un objet abandonné"
      : actions.find((a) => a.id === k)?.label ?? k.replace(/^omen_/, "Omen of ").replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
  const source = (k: string) => (ps && k in ps.overrides ? "manuel" : ps?.marketKeys.includes(k) ? "poe.ninja" : "exemple");

  const refresh = async () => {
    setBusy(true); setMsg(null);
    try {
      await api.setSettings(s); // prend en compte la ligue saisie
      const r = await api.refreshPrices();
      setPs(r); await reloadPrices();
      setMsg({ ok: true, text: r.note ?? `Prix actualisés depuis poe.ninja (ligue ${r.league}). Recalcule le plan pour les utiliser.` });
    } catch (e) { setMsg({ ok: false, text: String(e) }); } finally { setBusy(false); }
  };
  const saveOverrides = async (next: Record<string, number>) => {
    try { await api.setPrices(next); setPs(await api.priceState()); await reloadPrices(); setEdit({}); setMsg({ ok: true, text: "Prix enregistrés. Recalcule le plan pour les utiliser." }); }
    catch (e) { setMsg({ ok: false, text: String(e) }); }
  };
  const commit = () => {
    if (!ps) return;
    const next = { ...ps.overrides };
    for (const [k, v] of Object.entries(edit)) { const n = Number(v.replace(",", ".")); if (Number.isFinite(n) && n >= 0) next[k] = n; }
    void saveOverrides(next);
  };
  const keys = ps ? Object.keys(ps.effective).sort((a, b) => (a.startsWith("base_") === b.startsWith("base_") ? label(a).localeCompare(label(b)) : a.startsWith("base_") ? -1 : 1)) : [];

  return (
    <div className="panel pad stack" style={{ gap: 10 }}>
      <div className="row"><h3 className="hd">Prix</h3><span className="muted small">en {unit} (Exalted Orb)</span></div>
      <div className="row" style={{ alignItems: "flex-end" }}>
        <label className="f grow">Ligue poe.ninja (vide = ligue courante détectée automatiquement)
          <input type="text" value={s.priceLeague} placeholder="ex. Forbidden Rites" onChange={(e) => set("priceLeague", e.target.value)} /></label>
        <button className="btn primary" disabled={busy} onClick={() => void refresh()}>{busy ? "Actualisation…" : "Actualiser depuis poe.ninja"}</button>
      </div>
      <label className="row small"><input type="checkbox" checked={s.autoRefreshPrices} onChange={(e) => set("autoRefreshPrices", e.target.checked)} /> Actualiser automatiquement au démarrage si les prix ont plus d'une heure (enregistré avec « Enregistrer »)</label>
      <p className="small muted">
        {ps?.fetchedAt ? <>Dernière actualisation : <b>{ago(ps.now - ps.fetchedAt)}</b>, ligue <b>{ps.league}</b>.</> : "Pas encore de prix du marché : les prix d'exemple du jeu de données sont utilisés."}
        {" "}poe.ninja met ses prix à jour environ toutes les heures ; l'app espace ses requêtes d'au moins 5 minutes.
      </p>
      {msg && <div className={msg.ok ? "note small" : "err"}>{msg.text}</div>}
      {ps && ps.missing.length > 0 && <div className="note small">Sans prix récent sur poe.ninja (prix par défaut conservé) : {ps.missing.map(label).join(", ")}.</div>}
      <div style={{ maxHeight: 460, overflow: "auto" }}>
        <table className="t">
          <thead><tr><th>Objet</th><th>Origine</th><th className="n" style={{ width: 120 }}>Prix ({unit})</th><th style={{ width: 90 }} /></tr></thead>
          <tbody>
            {keys.map((k) => (
              <tr key={k}>
                <td>{label(k)}</td>
                <td><span className={`pill ${source(k) === "manuel" ? "prefix" : ""}`}>{source(k)}</span></td>
                <td className="n"><input type="text" inputMode="decimal" style={{ width: 100, textAlign: "right" }} value={edit[k] ?? String(Math.round(ps!.effective[k] * 100) / 100)} onChange={(e) => setEdit({ ...edit, [k]: e.target.value })} /></td>
                <td>{ps && k in ps.overrides && <button className="btn ghost sm" title="Revenir au prix du marché ou d'exemple" onClick={() => { const n = { ...ps.overrides }; delete n[k]; void saveOverrides(n); }}>Réinitialiser</button>}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="row"><button className="btn" disabled={Object.keys(edit).length === 0} onClick={commit}>Enregistrer mes prix</button>
        <span className="small muted">Un prix saisi à la main prime sur poe.ninja ; « Réinitialiser » le retire.</span></div>
    </div>
  );
}

export function SettingsPage() {
  const [s, setS] = useState<Settings | null>(null);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [hkError, setHkError] = useState<string | null>(null);
  useEffect(() => { void api.getSettings().then(setS); void api.hotkeyStatus().then(setHkError); }, []);
  if (!s) return null;
  const set = <K extends keyof Settings>(k: K, v: Settings[K]) => setS({ ...s, [k]: v });
  const save = async () => {
    try { await api.setSettings(s); setMsg({ ok: true, text: "Réglages enregistrés." }); setHkError(await api.hotkeyStatus()); } catch (e) { setMsg({ ok: false, text: String(e) }); }
  };
  return (
    <div className="page">
      <div className="page-head"><h1>Réglages</h1></div>
      <div className="stack" style={{ maxWidth: 820 }}>
        <UpdatesPanel s={s} set={set} />
        <div className="panel pad stack">
          <h3 className="hd">Raccourcis globaux</h3>
          {hkError && <div className="err">Les raccourcis ne fonctionnent pas : {hkError}</div>}
          <div className="row">
            <label className="f grow">Afficher / masquer l'overlay<input type="text" value={s.hotkeyToggle} onChange={(e) => set("hotkeyToggle", e.target.value)} /></label>
            <label className="f grow">Mode interactif (l'overlay capte la souris)<input type="text" value={s.hotkeyInteractive} onChange={(e) => set("hotkeyInteractive", e.target.value)} /></label>
          </div>
          <p className="note small">Exemples : « Ctrl+D », « Ctrl+Shift+D », « F9 ». Sans raccourci, l'icône de l'application (zone de notification, près de l'horloge) permet d'afficher ou masquer l'overlay et de quitter. Un raccourci global masque cette combinaison pour les autres applications (Ctrl+D ajoute un favori dans un navigateur, par exemple) : choisis-en un que tu n'utilises pas ailleurs pendant que l'app tourne.</p>
        </div>
        <div className="panel pad stack">
          <h3 className="hd">Presse-papiers et overlay</h3>
          <label className="row"><input type="checkbox" checked={s.watchClipboard} onChange={(e) => set("watchClipboard", e.target.checked)} /> Analyser automatiquement chaque objet copié</label>
          <label className="row"><input type="checkbox" checked={s.autoShowOnCopy} onChange={(e) => set("autoShowOnCopy", e.target.checked)} /> Afficher l'overlay quand un objet est copié</label>
          <label className="f" style={{ maxWidth: 420 }}>Masquer l'overlay après une copie (secondes, 0 = jamais)
            <input type="number" min={0} max={120} value={s.overlayAutoHideSecs} onChange={(e) => set("overlayAutoHideSecs", Math.max(0, +e.target.value || 0))} /></label>
          <div className="row">
            <label className="f grow">Largeur (px)<input type="number" value={s.overlayWidth} min={300} max={800} onChange={(e) => set("overlayWidth", +e.target.value)} /></label>
            <label className="f grow">Hauteur (px)<input type="number" value={s.overlayHeight} min={300} max={1400} onChange={(e) => set("overlayHeight", +e.target.value)} /></label>
            <label className="f grow">Marge droite<input type="number" value={s.overlayMarginX} onChange={(e) => set("overlayMarginX", +e.target.value)} /></label>
            <label className="f grow">Marge haute<input type="number" value={s.overlayMarginY} onChange={(e) => set("overlayMarginY", +e.target.value)} /></label>
          </div>
          <label className="f">Titre de la fenêtre du jeu<input type="text" value={s.gameWindowTitle} onChange={(e) => set("gameWindowTitle", e.target.value)} /></label>
          <p className="note small">L'overlay se superpose au jeu en mode fenêtré sans bordure ou fenêtré. En plein écran exclusif, Windows ne permet pas d'afficher une fenêtre par-dessus : passe le jeu en « fenêtré sans bordure ».</p>
        </div>
        <div className="panel pad stack">
          <h3 className="hd">Calcul</h3>
          <div className="row">
            <label className="f" style={{ width: 240 }}>Threads de calcul (0 = automatique)<input type="number" min={0} max={64} value={s.cpuThreads} onChange={(e) => set("cpuThreads", +e.target.value)} /></label>
            <label className="f" style={{ width: 200 }}>Niveau d'objet par défaut<input type="number" min={1} max={100} value={s.defaultIlvl} onChange={(e) => set("defaultIlvl", +e.target.value)} /></label>
          </div>
          <p className="note small">Les calculs longs tournent sur un pool dédié qui laisse au moins deux cœurs libres, pour ne pas faire ramer le jeu.</p>
        </div>
        <div className="row"><button className="btn primary" onClick={() => void save()}>Enregistrer</button>{msg && <span className={msg.ok ? "ok-t" : ""}>{msg.ok ? msg.text : <span className="err">{msg.text}</span>}</span>}</div>
        <PricesEditor s={s} set={set} />
      </div>
    </div>
  );
}
