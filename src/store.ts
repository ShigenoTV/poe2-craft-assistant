import { create } from "zustand";
import { api, listen } from "@/lib/ipc";
import type { ActionView, CraftPlan, DatasetInfo, PoolView, Progress, UpdateInfo, WantedReq } from "@/lib/types";

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
  setDatasetInfo: (i: DatasetInfo) => Promise<void>;

  // formulaire du planificateur (persiste quand on change de page)
  baseId: string;
  ilvl: number;
  wanted: WantedReq[];
  enabled: string[];
  activate: boolean;
  mcTrials: number;
  setPlanner: (p: Partial<Pick<Store, "baseId" | "ilvl" | "wanted" | "enabled" | "activate" | "mcTrials">>) => void;

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
    try {
      const [info, actions, prices, settings] = await Promise.all([api.datasetInfo(), api.listActions(), api.getPrices(), api.getSettings()]);
      const baseId = info.bases[0]?.id ?? "";
      set({ info, actions, prices, baseId, ilvl: settings.defaultIlvl, enabled: actions.filter((a) => a.defaultEnabled).map((a) => a.id) });
      if (baseId) await get().ensurePool(baseId);
      set({ ready: true });
      void api.appVersion().then((version) => set({ version }));
      void listen("prices-updated", () => void get().reloadPrices());
      if (settings.checkUpdatesOnStart) void get().checkUpdate(false);
    } catch (e) {
      set({ bootError: String(e), ready: true });
    }
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
  setDatasetInfo: async (info) => {
    const [actions, prices] = await Promise.all([api.listActions(), api.getPrices()]);
    const baseId = info.bases[0]?.id ?? "";
    set({ info, actions, prices, pools: {}, baseId, wanted: [], plan: null, enabled: actions.filter((a) => a.defaultEnabled).map((a) => a.id) });
    if (baseId) await get().ensurePool(baseId);
  },

  baseId: "",
  ilvl: 80,
  wanted: [],
  enabled: [],
  activate: true,
  mcTrials: 20000,
  setPlanner: (p) => set(p),

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
        { baseId: s.baseId, ilvl: s.ilvl, wanted: s.wanted, enabledActions: s.enabled, allowAbandon: true, mcTrials: s.mcTrials, nodeCap: 220, seed: 42 },
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
