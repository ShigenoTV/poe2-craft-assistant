/** Unité d'affichage des coûts. Le moteur et les prix restent en Exalted : seule la présentation change. */
export type CostUnit = "ex" | "chaos" | "div";
export const COST_UNITS: readonly CostUnit[] = ["ex", "chaos", "div"];

/** Prix en Exalted des unités autres que l'Exalted (Chaos Orb, Divine Orb) ; `null` = inconnu. */
export type UnitRates = Record<Exclude<CostUnit, "ex">, number | null>;

const usable = (r: number | null): r is number => r !== null && Number.isFinite(r) && r > 0;

/** Prix en Exalted d'une unité ; `null` si inconnu (l'affichage reste alors en Exalted). */
export const rateOf = (unit: CostUnit, rates: UnitRates): number | null => (unit === "ex" ? 1 : usable(rates[unit]) ? rates[unit] : null);

/** Montant en Exalted → montant à afficher, `rate` = prix en Exalted de l'unité choisie. Un montant inférieur à 1 unité
 * reste en Exalted (« 304 ex » se lit mieux que « 0,65 div ») ; sans prix connu, tout reste en Exalted. */
export function toDisplay(ex: number, unit: CostUnit, rate: number | null): { value: number; unit: CostUnit } {
  if (unit !== "ex" && usable(rate) && Math.abs(ex) >= rate) return { value: ex / rate, unit };
  return { value: ex, unit: "ex" };
}

/** Saisie dans l'unité choisie (champ « J'ai » du budget) → Exalted. */
export function inputToExalted(v: number, unit: CostUnit, rate: number | null): number {
  return unit !== "ex" && usable(rate) ? v * rate : v;
}

/** Montant en Exalted → valeur d'un champ de saisie dans l'unité choisie (sans le seuil d'1 unité de `toDisplay`). */
export function exaltedToInput(ex: number, unit: CostUnit, rate: number | null): number {
  return unit !== "ex" && usable(rate) ? ex / rate : ex;
}

export const isCostUnit = (u: unknown): u is CostUnit => u === "ex" || u === "chaos" || u === "div";

/** Écart relatif du coût réel au coût prévu (historique des crafts) ; `null` sans prévision. */
export function gapOf(planned: number, real: number): number | null {
  return Number.isFinite(planned) && planned > 0 && Number.isFinite(real) ? (real - planned) / planned : null;
}
