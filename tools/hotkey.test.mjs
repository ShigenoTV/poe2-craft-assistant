// Capture d'un raccourci au clavier (src/lib/hotkey.ts). Lancer : npm test
import { test } from "node:test";
import assert from "node:assert/strict";
import { hotkeyFromEvent } from "../src/lib/hotkey.ts";

const press = (key, code, mods = {}) => ({ key, code, ctrlKey: false, shiftKey: false, altKey: false, metaKey: false, ...mods });

test("Ctrl+S et Ctrl+Shift+S donnent le format des réglages", () => {
  assert.equal(hotkeyFromEvent(press("s", "KeyS", { ctrlKey: true })), "Ctrl+S");
  assert.equal(hotkeyFromEvent(press("S", "KeyS", { ctrlKey: true, shiftKey: true })), "Ctrl+Shift+S");
});

test("une touche de modification seule n'est pas un raccourci", () => {
  assert.equal(hotkeyFromEvent(press("Control", "ControlLeft", { ctrlKey: true })), null);
  assert.equal(hotkeyFromEvent(press("Shift", "ShiftLeft", { ctrlKey: true, shiftKey: true })), null);
});

test("AZERTY : la lettre suit la disposition, le chiffre suit la position", () => {
  // touche marquée « A » d'un AZERTY = position Q d'un QWERTY
  assert.equal(hotkeyFromEvent(press("a", "KeyQ", { ctrlKey: true })), "Ctrl+A");
  // touche « & / 1 » de la rangée du haut
  assert.equal(hotkeyFromEvent(press("&", "Digit1", { altKey: true })), "Alt+Digit1");
});

test("touches de fonction et pavé numérique", () => {
  assert.equal(hotkeyFromEvent(press("F9", "F9")), "F9");
  assert.equal(hotkeyFromEvent(press("+", "NumpadAdd", { ctrlKey: true })), "Ctrl+NumpadAdd");
});

test("une touche inconnue est refusée", () => {
  assert.equal(hotkeyFromEvent(press("Unidentified", "IntlBackslash")), null);
});
