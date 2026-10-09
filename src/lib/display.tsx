import { create } from "zustand";
import { api, listen } from "@/lib/ipc";
import { cost } from "@/lib/format";
import { COST_UNITS, isCostUnit, rateOf, toDisplay, type CostUnit, type UnitRates } from "@/lib/money";

/** Unité d'affichage des coûts (Exalted, Chaos ou Divine), partagée par la fenêtre principale et l'overlay :
 * le choix est enregistré dans les réglages, le prix du Chaos et de la Divine vient des prix (poe.ninja ou saisie manuelle). */
interface Display {
  unit: CostUnit;
  /** prix du Chaos et de la Divine en Exalted ; `null` = inconnu (tout reste affiché en Exalted) */
  rates: UnitRates;
  init: () => void;
  setUnit: (u: CostUnit) => Promise<void>;
}

let started = false;
const loadRates = async () => {
  const p = await api.getPrices();
  useDisplay.setState({ rates: { chaos: p.chaos ?? null, div: p.divine ?? null } });
};

export const useDisplay = create<Display>((set) => ({
  unit: "ex",
  rates: { chaos: null, div: null },
  init: () => {
    if (started) return;
    started = true;
    void api.getSettings().then((s) => set({ unit: isCostUnit(s.costUnit) ? s.costUnit : "ex" }));
    void loadRates();
    void listen<{ costUnit?: string }>("settings-changed", (s) => { if (isCostUnit(s.costUnit)) set({ unit: s.costUnit }); });
    void listen("prices-updated", () => void loadRates());
    void listen("plan-refreshed", () => void loadRates());
  },
  setUnit: async (unit) => {
    set({ unit });
    const s = await api.getSettings();
    await api.setSettings({ ...s, costUnit: unit });
  },
}));

/** À rappeler après une saisie manuelle de prix (le Chaos ou la Divine a pu changer). */
export const reloadRates = () => void loadRates();

const UNIT_NAME: Record<CostUnit, string> = { ex: "Exalted", chaos: "Chaos", div: "Divine" };

/** Formateur de coûts (montant en Exalted) dans l'unité choisie ; le composant se redessine quand l'unité change. */
export function useCost(): (ex: number) => string {
  const { unit, rates } = useDisplay();
  const rate = rateOf(unit, rates);
  return (ex) => { const d = toDisplay(ex, unit, rate); return cost(d.value, d.unit); };
}

/** Coût affiché dans l'unité choisie ; le survol montre toujours le montant en Exalted. */
export function Cost({ ex }: { ex: number }) {
  const { unit, rates } = useDisplay();
  const d = toDisplay(ex, unit, rateOf(unit, rates));
  return d.unit === "ex" ? <>{cost(ex)}</> : <span title={cost(ex)}>{cost(d.value, d.unit)}</span>;
}

/** Sélecteur « Exalted | Chaos | Divine ». */
export function CostUnitSwitch() {
  const { unit, rates, setUnit } = useDisplay();
  const tip = (u: CostUnit) => {
    if (u === "ex") return "Coûts en Exalted";
    const r = rateOf(u, rates);
    return r ? `1 ${UNIT_NAME[u]} = ${cost(r)} (prix des Réglages)` : `Prix du ${UNIT_NAME[u]} inconnu : les coûts restent en Exalted`;
  };
  return (
    <div className="seg" role="group" aria-label="Unité des coûts">
      {COST_UNITS.map((u) => (
        <button key={u} aria-pressed={unit === u} title={tip(u)} onClick={() => void setUnit(u)}>{UNIT_NAME[u]}</button>
      ))}
    </div>
  );
}
