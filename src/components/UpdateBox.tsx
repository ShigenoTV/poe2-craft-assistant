import { useStore } from "@/store";
import { num } from "@/lib/format";

/** Bandeau discret dans la barre latérale : n'apparaît que s'il y a quelque chose à faire. */
export function UpdateBox() {
  const { version, update, updateStatus, updateProgress, updateError, installUpdate, checkUpdate, setPage } = useStore();
  if (updateStatus === "installing") {
    const pctDone = updateProgress?.total ? updateProgress.downloaded / updateProgress.total : 0;
    return (
      <div className="update-box" role="status">
        <b>Mise à jour en cours</b>
        <div className={`progress ${pctDone === 0 ? "ind" : ""}`}><i style={{ width: `${pctDone * 100}%` }} /></div>
        <span className="small muted">{updateProgress?.total ? `${num((updateProgress.downloaded / 1e6), 1)} / ${num(updateProgress.total / 1e6, 1)} Mo` : "Installation…"}</span>
      </div>
    );
  }
  if (updateStatus !== "available" || !update) {
    // Toujours visible : la recherche manuelle ne doit pas être cachée tant qu'aucune mise à jour n'est annoncée.
    return (
      <div className="update-line">
        <span className="small muted">Version {version || "…"}</span>
        <button className="btn sm" disabled={updateStatus === "checking"} onClick={() => void checkUpdate(true)}>
          {updateStatus === "checking" ? "Recherche…" : "Rechercher une mise à jour"}
        </button>
        {updateStatus === "uptodate" && <span className="small ok-t">Tu as la dernière version.</span>}
        {updateStatus === "unconfigured" && <button className="btn ghost sm" onClick={() => setPage("settings")}>Non configurées : voir Réglages</button>}
        {updateStatus === "error" && <span className="small bad-t" title={updateError ?? ""}>Échec de la recherche (détails dans Réglages).</span>}
      </div>
    );
  }
  return (
    <div className="update-box" role="status">
      <b>Version {update.version} disponible</b>
      <span className="small muted">Tu utilises la {update.current}.</span>
      <div className="row">
        <button className="btn primary sm" onClick={() => void installUpdate()}>Installer et redémarrer</button>
        <button className="btn ghost sm" onClick={() => setPage("settings")}>Détails</button>
      </div>
    </div>
  );
}
