/** Unité d'affichage des coûts. Le moteur et les prix restent en Exalted : seule la présentation change. */
export type CostUnit = "ex" | "div";

/** Montant en Exalted → montant à afficher. En Divine, un montant inférieur à 1 Divine reste en Exalted
 * (« 304 ex » se lit mieux que « 0,65 div ») ; sans prix de la Divine connu, tout reste en Exalted. */
export function toDisplay(ex: number, unit: CostUnit, divine: number | null): { value: number; unit: CostUnit } {
  if (unit === "div" && divine !== null && Number.isFinite(divine) && divine > 0 && Math.abs(ex) >= divine) return { value: ex / divine, unit: "div" };
  return { value: ex, unit: "ex" };
}

/** Saisie dans l'unité choisie (champ « J'ai » du budget) → Exalted. */
export function inputToExalted(v: number, unit: CostUnit, divine: number | null): number {
  return unit === "div" && divine !== null && divine > 0 ? v * divine : v;
}

/** Montant en Exalted → valeur d'un champ de saisie dans l'unité choisie (sans le seuil d'1 Divine de `toDisplay`). */
export function exaltedToInput(ex: number, unit: CostUnit, divine: number | null): number {
  return unit === "div" && divine !== null && divine > 0 ? ex / divine : ex;
}

export const isCostUnit = (u: unknown): u is CostUnit => u === "ex" || u === "div";

/** Écart relatif du coût réel au coût prévu (historique des crafts) ; `null` sans prévision. */
export function gapOf(planned: number, real: number): number | null {
  return Number.isFinite(planned) && planned > 0 && Number.isFinite(real) ? (real - planned) / planned : null;
}
