import type { ReactNode } from "react";
import { prettyText } from "@/lib/format";
import type { BaseView } from "@/lib/types";

// catégories d'objet (item_class du jeu) : libellé affiché et famille pour regrouper la liste
const CLASSES: Record<string, [string, "Bijoux" | "Armures" | "Armes" | "Autres"]> = {
  Amulet: ["Amulette", "Bijoux"], Ring: ["Anneau", "Bijoux"], Belt: ["Ceinture", "Bijoux"], Jewel: ["Joyau", "Bijoux"],
  Helmet: ["Casque", "Armures"], "Body Armour": ["Armure de torse", "Armures"], Gloves: ["Gants", "Armures"], Boots: ["Bottes", "Armures"],
  Shield: ["Bouclier", "Armures"], Buckler: ["Targe", "Armures"], Focus: ["Focus", "Armures"], Quiver: ["Carquois", "Armures"],
  Claw: ["Griffe", "Armes"], Dagger: ["Dague", "Armes"], Wand: ["Baguette", "Armes"], Sceptre: ["Sceptre", "Armes"],
  "One Hand Sword": ["Épée à une main", "Armes"], "One Hand Axe": ["Hache à une main", "Armes"], "One Hand Mace": ["Masse à une main", "Armes"],
  "Two Hand Sword": ["Épée à deux mains", "Armes"], "Two Hand Axe": ["Hache à deux mains", "Armes"], "Two Hand Mace": ["Masse à deux mains", "Armes"],
  Bow: ["Arc", "Armes"], Crossbow: ["Arbalète", "Armes"], Staff: ["Bâton", "Armes"], Warstaff: ["Bâton de guerre", "Armes"],
  Spear: ["Lance", "Armes"], Flail: ["Fléau", "Armes"], Talisman: ["Talisman", "Armes"], TrapTool: ["Piège", "Armes"],
};
const FAMILIES = ["Bijoux", "Armures", "Armes", "Autres"] as const;
const classLabel = (c: string) => CLASSES[c]?.[0] ?? c;
const baseOption = (b: BaseView) => `${b.name}${b.implicits?.length ? ` — ${prettyText(b.implicits.join(", "))}` : ""}`;

interface Props {
  bases: BaseView[];
  value: string;
  onChange: (baseId: string) => void;
  /** placé à côté du choix de catégorie (ex. niveau d'objet) */
  aside?: ReactNode;
}

/** Choix en deux temps : catégorie d'objet, puis la base précise de cette catégorie (avec son implicite). */
export function BaseSelect({ bases, value, onChange, aside }: Props) {
  const current = bases.find((b) => b.id === value);
  const cls = current?.itemClass ?? bases[0]?.itemClass ?? "";
  const classes = [...new Set(bases.map((b) => b.itemClass))];
  const inClass = bases.filter((b) => b.itemClass === cls).sort((a, b) => a.name.localeCompare(b.name));
  return (
    <>
      <div className="row">
        <label className="f grow">Catégorie
          <select value={cls} onChange={(e) => { const first = bases.filter((b) => b.itemClass === e.target.value).sort((a, b) => a.name.localeCompare(b.name))[0]; if (first) onChange(first.id); }}>
            {FAMILIES.map((fam) => {
              const list = classes.filter((c) => (CLASSES[c]?.[1] ?? "Autres") === fam).sort((a, b) => classLabel(a).localeCompare(classLabel(b), "fr"));
              return list.length ? <optgroup key={fam} label={fam}>{list.map((c) => <option key={c} value={c}>{classLabel(c)}</option>)}</optgroup> : null;
            })}
          </select>
        </label>
        {aside}
      </div>
      <label className="f">Base ({inClass.length})
        <select value={value} onChange={(e) => onChange(e.target.value)}>
          {inClass.map((b) => <option key={b.id} value={b.id}>{baseOption(b)}</option>)}
        </select>
      </label>
    </>
  );
}
