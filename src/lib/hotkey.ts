/** Touche pressée, réduite à ce qu'il faut pour en faire un raccourci (sous-ensemble de `KeyboardEvent`). */
export interface KeyPress { key: string; code: string; ctrlKey: boolean; shiftKey: boolean; altKey: boolean; metaKey: boolean }

const MODIFIER_KEYS = new Set(["Control", "Shift", "Alt", "Meta", "AltGraph", "OS"]);

/** Touches (`KeyboardEvent.code`) reprises telles quelles : le service de raccourcis globaux les comprend sous ce nom. */
const NAMED = /^(F([1-9]|1[0-2])|Digit[0-9]|Numpad([0-9]|Add|Subtract|Multiply|Divide|Decimal|Enter)|Arrow(Up|Down|Left|Right)|Space|Enter|Tab|Backspace|Delete|Insert|Home|End|PageUp|PageDown|Pause|ScrollLock|PrintScreen|Backquote|Minus|Equal|BracketLeft|BracketRight|Backslash|Semicolon|Quote|Comma|Period|Slash)$/;

/**
 * Combinaison au format des réglages (« Ctrl+Shift+S », « F9 »), ou `null` tant que seule une touche de modification
 * est enfoncée ou que la touche n'est pas utilisable comme raccourci global.
 * Les lettres suivent la disposition du clavier (la touche marquée « A » sur un AZERTY donne « A », comme le
 * raccourci Windows) ; chiffres, touches F et touches spéciales suivent leur position.
 */
export function hotkeyFromEvent(e: KeyPress): string | null {
  if (MODIFIER_KEYS.has(e.key)) return null;
  let main: string | null = null;
  if (/^[a-z]$/i.test(e.key)) main = e.key.toUpperCase();
  else if (/^Key[A-Z]$/.test(e.code)) main = e.code.slice(3);
  else if (NAMED.test(e.code)) main = e.code;
  if (!main) return null;
  const mods = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Super"].filter(Boolean);
  return [...mods, main].join("+");
}
