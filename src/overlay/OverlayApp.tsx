import { useEffect, useState } from "react";
import { api, listen } from "@/lib/ipc";
import { AdviceView } from "@/components/AdviceView";
import { prettyText, rarityLabel } from "@/lib/format";
import type { ActiveInfo, ItemCaptured } from "@/lib/types";

export function OverlayApp() {
  const [cap, setCap] = useState<ItemCaptured | null>(null);
  const [interactive, setInteractive] = useState(false);
  const [active, setActive] = useState<ActiveInfo | null>(null);
  const [hkError, setHkError] = useState<string | null>(null);

  useEffect(() => {
    void api.overlayState().then(([, i]) => setInteractive(i));
    void api.hotkeyStatus().then(setHkError);
    const refresh = () => void api.activePlan().then(setActive);
    refresh();
    const offs: (() => void)[] = [];
    void listen<ItemCaptured>("item-captured", (c) => { setCap(c); refresh(); }).then((f) => offs.push(f));
    void listen<boolean>("overlay-interactive", setInteractive).then((f) => offs.push(f));
    return () => offs.forEach((f) => f());
  }, []);

  const a = cap?.analysis;
  const rarity = a?.parsed.rarity ?? "normal";
  return (
    <div className="ov">
      <div className="ov-card">
        <div className="ov-top">
          <span className={`dot ${active ? "live" : ""}`} title={active ? "Plan actif" : "Aucun plan actif"} />
          <span className="grow">{active ? `Plan : ${active.goal.length} affixes voulus` : "Aucun plan actif"}</span>
          <span>{interactive ? "interactif" : "clic-traversant"}</span>
          {interactive && (
            <>
              <button className="btn sm" onClick={() => void api.overlaySetInteractive(false)}>Verrouiller</button>
              <button className="btn sm" onClick={() => void api.overlayToggle()}>Fermer</button>
            </>
          )}
        </div>
        <div className="ov-body">
          {hkError && <div className="ov-warn">Raccourcis inactifs : utilise l'icône de l'application près de l'horloge pour masquer cet overlay.</div>}
          {!cap && <p className="muted small">Survole un objet en jeu et copie-le (<span className="kbd">Ctrl+Alt+C</span>) : le conseil apparaît ici.</p>}
          {a?.error && <div className="ov-warn">{a.error}</div>}
          {a && !a.error && (
            <>
              <div className={`ov-name ${rarity}`}>
                <b>{a.parsed.name ?? "Objet"}</b>
                <span>{rarityLabel[rarity]} · {a.parsed.baseType} · niv. {a.parsed.itemLevel ?? "?"}</span>
              </div>
              {a.detail && (
                <div>
                  {a.detail.mods.map((m) => (
                    <div key={m.affixIdx} className="mod" style={{ padding: "3px 0" }}>
                      <span className="tier">T{m.tier}</span><span className="tx">{prettyText(m.text)}</span>{m.fractured && <span className="lock">◆</span>}
                    </div>
                  ))}
                </div>
              )}
              {a.unmatched.length > 0 && <div className="ov-warn">{a.unmatched.length} ligne(s) non reconnue(s)</div>}
              {cap && <AdviceView cap={cap} compact />}
            </>
          )}
        </div>
      </div>
    </div>
  );
}
