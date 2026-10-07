import { create } from "zustand";
import { api, listen } from "@/lib/ipc";
import { cost } from "@/lib/format";
import { isCostUnit, toDisplay, type CostUnit } from "@/lib/money";

/** Unité d'affichage des coûts (Exalted ou Divine), partagée par la fenêtre principale et l'overlay :
 * le choix est enregistré dans les réglages, le prix de la Divine vient des prix (poe.ninja ou saisie manuelle). */
interface Display {
  unit: CostUnit;
  /** prix d'une Divine en Exalted ; `null` = inconnu (tout reste affiché en Exalted) */
  divine: number | null;
  init: () => void;
  setUnit: (u: CostUnit) => Promise<void>;
}

let started = false;
const loadDivine = async () => {
  const p = await api.getPrices();
  useDisplay.setState({ divine: p.divine ?? null });
};

export const useDisplay = create<Display>((set) => ({
  unit: "ex",
  divine: null,
  init: () => {
    if (started) return;
    started = true;
    void api.getSettings().then((s) => set({ unit: isCostUnit(s.costUnit) ? s.costUnit : "ex" }));
    void loadDivine();
    void listen<{ costUnit?: string }>("settings-changed", (s) => { if (isCostUnit(s.costUnit)) set({ unit: s.costUnit }); });
    void listen("prices-updated", () => void loadDivine());
    void listen("plan-refreshed", () => void loadDivine());
  },
  setUnit: async (unit) => {
    set({ unit });
    const s = await api.getSettings();
    await api.setSettings({ ...s, costUnit: unit });
  },
}));

/** À rappeler après une saisie manuelle de prix (la Divine a pu changer). */
export const reloadDivine = () => void loadDivine();

/** Formateur de coûts (montant en Exalted) dans l'unité choisie ; le composant se redessine quand l'unité change. */
export function useCost(): (ex: number) => string {
  const { unit, divine } = useDisplay();
  return (ex) => { const d = toDisplay(ex, unit, divine); return cost(d.value, d.unit); };
}

/** Coût affiché dans l'unité choisie ; le survol montre toujours le montant en Exalted. */
export function Cost({ ex }: { ex: number }) {
  const { unit, divine } = useDisplay();
  const d = toDisplay(ex, unit, divine);
  return d.unit === "ex" ? <>{cost(ex)}</> : <span title={cost(ex)}>{cost(d.value, d.unit)}</span>;
}

/** Sélecteur « ex | div ». */
export function CostUnitSwitch() {
  const { unit, divine, setUnit } = useDisplay();
  const tip = divine ? `1 Divine = ${cost(divine)} (prix des Réglages)` : "Prix de la Divine inconnu : les coûts restent en Exalted";
  return (
    <div className="seg" role="group" aria-label="Unité des coûts" title={tip}>
      {(["ex", "div"] as const).map((u) => (
        <button key={u} aria-pressed={unit === u} onClick={() => void setUnit(u)}>{u === "ex" ? "Exalted" : "Divine"}</button>
      ))}
    </div>
  );
}
