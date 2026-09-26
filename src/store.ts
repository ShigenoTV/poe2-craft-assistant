import { create } from "zustand";
import { api, listen } from "@/lib/ipc";
import type { ActionView, CraftPlan, DatasetInfo, ItemAnalysis, ItemView, PoolView, Progress, UpdateInfo, WantedReq } from "@/lib/types";

export type Page = "planner" | "sandbox" | "item" | "data" | "settings";

interface Store {
  page: Page;
  setPage: (p: Page) => void;

  info: DatasetInfo | null;
  pools: Record<string, PoolView>;
  actions: ActionView[];
  prices: Record<string, number>;
  ready: boolean;
  bootError: string | null;
  boot: () => Promise<void>;
  ensurePool: (baseId: string) => Promise<PoolView>;
  reloadPrices: () => Promise<void>;

  // formulaire du planificateur (persiste quand on change de page)
  baseId: string;
  ilvl: number;
  wanted: WantedReq[];
  enabled: string[];
  activate: boolean;
  mcTrials: number;
  /** Objet déjà existant dont on repart (au lieu d'une base neuve) ; `null` = base neuve, comme avant. */
  startingItem: ItemView | null;
  startingItemAnalysis: ItemAnalysis | null;
  startingItemError: string | null;
  analyzingStartingItem: boolean;
  setPlanner: (p: Partial<Pick<Store, "baseId" | "ilvl" | "wanted" | "enabled" | "activate" | "mcTrials" | "startingItem">>) => void;
  analyzeStartingItem: (text: string) => Promise<void>;
  clearStartingItem: () => void;

  plan: CraftPlan | null;
  progress: Progress | null;
  solving: boolean;
  solveError: string | null;
  solve: () => Promise<void>;
  cancel: () => Promise<void>;

  version: string;
  update: UpdateInfo | null;
  updateStatus: "idle" | "checking" | "uptodate" | "available" | "installing" | "unconfigured" | "error";
  updateProgress: { downloaded: number; total: number | null } | null;
  updateError: string | null;
  /** `manual` : affiche aussi « à jour » et les erreurs ; sinon vérification silencieuse au démarrage. */
  checkUpdate: (manual: boolean) => Promise<void>;
  installUpdate: () => Promise<void>;
}

export const useStore = create<Store>((set, get) => ({
  page: "planner",
  setPage: (page) => set({ page }),

  info: null,
  pools: {},
  actions: [],
  prices: {},
  ready: false,
  bootError: null,
  boot: async () => {
    // Juste après le lancement, la fenêtre peut commencer à s'afficher avant que Tauri ait fini
    // d'enregistrer son état interne côté Rust (course au démarrage, plus probable sur une machine
    // lente ou un tout premier lancement) : quelques tentatives espacées suffisent à passer ce cap,
    // sans laisser l'utilisateur bloqué sur une erreur qui se serait résolue une seconde plus tard.
    const delays = [150, 300, 600, 1200, 2400];
    let lastError: unknown;
    for (let attempt = 0; attempt <= delays.length; attempt++) {
      try {
        const [info, actions, prices, settings] = await Promise.all([api.datasetInfo(), api.listActions(), api.getPrices(), api.getSettings()]);
        const baseId = info.bases[0]?.id ?? "";
        set({ info, actions, prices, baseId, ilvl: settings.defaultIlvl, enabled: actions.filter((a) => a.defaultEnabled).map((a) => a.id) });
        if (baseId) await get().ensurePool(baseId);
        set({ ready: true });
        void api.appVersion().then((version) => set({ version }));
        void listen("prices-updated", () => void get().reloadPrices());
        if (settings.checkUpdatesOnStart) void get().checkUpdate(false);
        return;
      } catch (e) {
        lastError = e;
        if (attempt < delays.length) await new Promise((r) => setTimeout(r, delays[attempt]));
      }
    }
    set({ bootError: String(lastError), ready: true });
  },
  ensurePool: async (baseId) => {
    const have = get().pools[baseId];
    if (have) return have;
    const p = await api.basePool(baseId);
    set((s) => ({ pools: { ...s.pools, [baseId]: p } }));
    return p;
  },
  reloadPrices: async () => {
    const [prices, actions] = await Promise.all([api.getPrices(), api.listActions()]);
    set({ prices, actions });
  },

  baseId: "",
  ilvl: 80,
  wanted: [],
  enabled: [],
  activate: true,
  mcTrials: 20000,
  startingItem: null,
  startingItemAnalysis: null,
  startingItemError: null,
  analyzingStartingItem: false,
  setPlanner: (p) => set(p),
  analyzeStartingItem: async (text) => {
    set({ analyzingStartingItem: true, startingItemError: null });
    try {
      const a = await api.analyzeItemText(text, get().baseId, get().ilvl);
      if (a.error) { set({ startingItemError: a.error, analyzingStartingItem: false }); return; }
      if (!a.detail) { set({ startingItemError: "Base non reconnue — sélectionne-la à la main puis réessaie.", analyzingStartingItem: false }); return; }
      set({ startingItem: a.detail.view, startingItemAnalysis: a, startingItemError: null, analyzingStartingItem: false, baseId: a.baseId ?? get().baseId });
    } catch (e) {
      set({ startingItemError: String(e), analyzingStartingItem: false });
    }
  },
  clearStartingItem: () => set({ startingItem: null, startingItemAnalysis: null, startingItemError: null }),

  plan: null,
  progress: null,
  solving: false,
  solveError: null,
  solve: async () => {
    const s = get();
    if (s.solving) return;
    set({ solving: true, solveError: null, progress: { stage: "solving", done: 0, total: 0 } });
    try {
      const plan = await api.solvePlan(
        { baseId: s.baseId, ilvl: s.ilvl, wanted: s.wanted, enabledActions: s.enabled, allowAbandon: true, mcTrials: s.mcTrials, nodeCap: 220, seed: 42, startingItem: s.startingItem },
        s.activate,
        (progress) => set({ progress }),
      );
      set({ plan });
    } catch (e) {
      set({ solveError: String(e) });
    } finally {
      set({ solving: false, progress: null });
    }
  },
  cancel: async () => { await api.cancelJob(); },

  version: "",
  update: null,
  updateStatus: "idle",
  updateProgress: null,
  updateError: null,
  checkUpdate: async (manual) => {
    if (get().updateStatus === "installing") return;
    if (manual) set({ updateStatus: "checking", updateError: null });
    try {
      const [version, update] = await Promise.all([api.appVersion(), api.checkUpdate()]);
      set({ version, update, updateStatus: update ? "available" : manual ? "uptodate" : "idle" });
    } catch (e) {
      const msg = String(e);
      if (msg.includes("not_configured")) set({ updateStatus: "unconfigured" });
      else if (manual) set({ updateStatus: "error", updateError: msg });
      else set({ updateStatus: "idle" }); // hors ligne au démarrage : on ne dérange pas
    }
  },
  installUpdate: async () => {
    set({ updateStatus: "installing", updateProgress: { downloaded: 0, total: null }, updateError: null });
    try {
      await api.installUpdate((p) => set({ updateProgress: { downloaded: p.downloaded, total: p.total } }));
    } catch (e) {
      set({ updateStatus: "error", updateError: String(e), updateProgress: null });
    }
  },
}));
