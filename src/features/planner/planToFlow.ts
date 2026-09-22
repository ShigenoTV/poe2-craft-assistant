import type { Edge, Node } from "@xyflow/react";
import ELK from "elkjs/lib/elk-api";
import type { ELK as ELKApi } from "elkjs/lib/elk-api";
import workerUrl from "elkjs/lib/elk-worker.min.js?url";
import type { CraftPlan, CraftNode } from "@/lib/types";

export const NODE_W = 272;
export const NODE_H = 128;

export interface FlowData extends Record<string, unknown> { node: CraftNode; goalLen: number; isRoot: boolean }
export interface EdgeData extends Record<string, unknown> { loopback: boolean; kind: "success" | "failure"; p: number }

/**
 * Ne garde que les `cap` étapes les plus visitées (plus la racine et l'objectif), puis élague ce qui n'est plus
 * atteignable depuis la racine : un graphe complet de plusieurs centaines d'étapes est illisible.
 */
export function planToFlow(plan: CraftPlan, cap: number): { nodes: Node<FlowData>[]; edges: Edge<EdgeData>[]; hidden: number } {
  const all = Object.values(plan.nodes);
  const actions = all.filter((n) => n.kind === "action").sort((a, b) => b.expectedVisits - a.expectedVisits);
  const keep = new Set<string>([plan.rootId, ...all.filter((n) => n.kind === "terminal").map((n) => n.id), ...actions.slice(0, cap).map((n) => n.id)]);
  // accessibilité depuis la racine dans le sous-graphe conservé
  const reach = new Set<string>([plan.rootId]);
  const queue = [plan.rootId];
  while (queue.length) {
    const n = plan.nodes[queue.pop()!];
    if (n?.kind !== "action") continue;
    for (const b of n.branches) if (keep.has(b.to) && !reach.has(b.to)) { reach.add(b.to); queue.push(b.to); }
  }
  const nodes: Node<FlowData>[] = all.filter((n) => reach.has(n.id)).map((n) => ({
    id: n.id,
    type: n.kind === "terminal" ? "goal" : "action",
    position: { x: 0, y: 0 },
    data: { node: n, goalLen: plan.goal.length, isRoot: n.id === plan.rootId },
    draggable: false,
  }));
  const edges: Edge<EdgeData>[] = [];
  for (const n of all) {
    if (n.kind !== "action" || !reach.has(n.id)) continue;
    for (const b of n.branches) {
      if (!reach.has(b.to)) continue;
      edges.push({ id: b.id, source: n.id, target: b.to, type: "default", data: { loopback: b.loopback, kind: b.kind, p: b.probability } });
    }
  }
  return { nodes, edges, hidden: actions.length - nodes.filter((n) => n.type === "action").length };
}

// ELK tourne dans son propre Web Worker (fourni par elkjs) : ~200 nœuds ne gèlent jamais l'interface.
let elk: ELKApi | null = null;
const getElk = () => (elk ??= new ELK({ workerUrl }));

/** Positions ELK (graphe en couches, gauche → droite). Les arêtes de retour n'entrent pas dans le calcul. */
export async function layout(nodes: Node<FlowData>[], edges: Edge<EdgeData>[]): Promise<Record<string, { x: number; y: number }>> {
  const out = await getElk().layout({
    id: "root",
    layoutOptions: {
      "elk.algorithm": "layered", "elk.direction": "RIGHT",
      "elk.layered.spacing.nodeNodeBetweenLayers": "84", "elk.spacing.nodeNode": "26",
    },
    children: nodes.map((n) => ({ id: n.id, width: NODE_W, height: NODE_H })),
    edges: edges.filter((e) => !e.data?.loopback).map((e) => ({ id: e.id, sources: [e.source], targets: [e.target] })),
  });
  return Object.fromEntries((out.children ?? []).map((c) => [c.id, { x: c.x ?? 0, y: c.y ?? 0 }]));
}
