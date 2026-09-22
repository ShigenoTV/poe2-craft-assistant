import { useEffect } from "react";
import { useStore, type Page } from "@/store";
import { PlannerPage } from "@/features/planner/PlannerPage";
import { SandboxPage } from "@/features/sandbox/SandboxPage";
import { ItemPage } from "@/features/item/ItemPage";
import { DataPage } from "@/features/data/DataPage";
import { SettingsPage } from "@/features/settings/SettingsPage";
import { isTauri } from "@/lib/ipc";
import { UpdateBox } from "@/components/UpdateBox";

const NAV: [Page, string][] = [["planner", "Reverse-crafting"], ["sandbox", "Simulateur"], ["item", "Objet en jeu"], ["data", "Données"], ["settings", "Réglages"]];

function Mark() {
  return (
    <svg width="30" height="30" viewBox="0 0 30 30" aria-hidden="true">
      <path d="M15 1.5 28.5 15 15 28.5 1.5 15Z" fill="#171f22" stroke="#a58a34" strokeWidth="1.4" />
      <path d="M15 8 22 15 15 22 8 15Z" fill="#e7c65b" />
    </svg>
  );
}

export function App() {
  const { page, setPage, boot, ready, bootError, info } = useStore();
  useEffect(() => { void boot(); }, [boot]);
  const sample = info?.source.includes("placeholder");
  return (
    <div className="app">
      <nav className="rail" aria-label="Navigation principale">
        <div className="brand"><Mark /><div><b>Craft PoE2</b><small>assistant</small></div></div>
        <div className="nav">
          {NAV.map(([id, label]) => <button key={id} aria-current={page === id ? "page" : undefined} onClick={() => setPage(id)}>{label}</button>)}
        </div>
        <div className="rail-foot">
          <UpdateBox />
          {sample && <span className="badge-warn">Données d'exemple : poids et prix inventés. Importe un vrai jeu de données.</span>}
          {!isTauri && <p style={{ marginTop: 8 }}>Mode navigateur (moteur factice)</p>}
        </div>
      </nav>
      <main className="main">
        {!ready && <div className="empty">Chargement…</div>}
        {bootError && <div className="page"><div className="err">Impossible de charger les données : {bootError}</div></div>}
        {ready && !bootError && (
          <>
            {page === "planner" && <PlannerPage />}
            {page === "sandbox" && <SandboxPage />}
            {page === "item" && <ItemPage />}
            {page === "data" && <DataPage />}
            {page === "settings" && <SettingsPage />}
          </>
        )}
      </main>
    </div>
  );
}
