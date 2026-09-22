import { memo, useEffect, useMemo, useState } from "react";
import {
  Background, BackgroundVariant, Controls, Handle, MarkerType, MiniMap, Position, ReactFlow, ReactFlowProvider,
  type Edge, type Node, type NodeProps, useReactFlow,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import type { ActionNodeData, CraftPlan } from "@/lib/types";
import { cost, num, pct } from "@/lib/format";
import { layout, planToFlow, type EdgeData, type FlowData } from "./planToFlow";

const PLATE: Record<string, string> = {
  transmute: "#22303f", augment: "#22303f", regal: "#2a2a44", alchemy: "#3a3320", exalt: "#3b3a1f",
  chaos: "#3d2622", annul: "#3a2131", fracture: "#2f2b3d", abandon: "#2b2b2b",
};
const plateOf = (id: string) => PLATE[id.startsWith("__") ? "abandon" : id.split(/[_+]/)[0]] ?? "#22303a";

function pips(n: ActionNodeData, goal: FlowData["goalLen"], slots: ("prefix" | "suffix")[]) {
  return Array.from({ length: goal }, (_, k) => {
    const st = n.state.heldWanted.includes(k) ? "held" : n.state.blockedWanted.includes(k) ? "blocked" : "";
    const frac = n.state.fracturedWanted === k ? "frac" : "";
    return <i key={k} className={`pip ${slots[k] ?? "prefix"} ${st} ${frac}`} />;
  });
}

const ActionNode = memo(function ActionNode({ data, selected }: NodeProps<Node<FlowData>>) {
  const n = data.node as ActionNodeData;
  const plan = usePlan();
  const junk = n.state.badPrefixes + n.state.badSuffixes;
  return (
    <div className={`cnode ${selected ? "sel" : ""} ${data.isRoot ? "root" : ""}`}>
      <Handle type="target" position={Position.Left} style={{ opacity: 0 }} />
      <div className="cn-head" style={{ ["--plate" as string]: plateOf(n.action.id) }}>
        <span className="cn-title" title={n.action.label}>{n.action.label}</span>
        <span className="cn-vis" title="Passages moyens par craft">×{num(n.expectedVisits, n.expectedVisits < 1 ? 2 : 1)}</span>
      </div>
      <div className="cn-body">
        <div className="pips">{pips(n, data.goalLen, plan?.goal.map((g) => g.slot) ?? [])}</div>
        <div className="cn-meta">
          {n.state.rarity === "normal" ? "Normal" : n.state.rarity === "magic" ? "Magique" : "Rare"}
          {junk > 0 && ` · ${n.state.badPrefixes} préf. / ${n.state.badSuffixes} suff. inutiles`}
        </div>
      </div>
      <div className="cn-foot">
        <span>reste <b>{cost(n.costToGo)}</b></span>
        {n.repeat && <span title="Boucle sur le même état">~{num(n.repeat.expectedAttempts, 1)} essais</span>}
      </div>
      <Handle type="source" position={Position.Right} style={{ opacity: 0 }} />
    </div>
  );
});

const GoalNode = memo(function GoalNode({ data }: NodeProps<Node<FlowData>>) {
  return (
    <div className="cnode goal">
      <Handle type="target" position={Position.Left} style={{ opacity: 0 }} />
      <div><b>Objectif atteint</b><div className="muted small">{num(data.node.expectedVisits, 2)} fois par craft en moyenne</div></div>
    </div>
  );
});

// le plan courant est passé par contexte simple pour ne pas dupliquer la liste des slots dans chaque nœud
let currentPlan: CraftPlan | null = null;
const usePlan = () => currentPlan;
const nodeTypes = { action: ActionNode, goal: GoalNode };

function Inner({ plan, cap, onSelect, selectedId, showLoops }: { plan: CraftPlan; cap: number; onSelect: (id: string | null) => void; selectedId: string | null; showLoops: boolean }) {
  const { nodes: n0, edges: e0 } = useMemo(() => planToFlow(plan, cap), [plan, cap]);
  const [pos, setPos] = useState<Record<string, { x: number; y: number }> | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const rf = useReactFlow();
  currentPlan = plan;

  useEffect(() => {
    let live = true;
    setPos(null); setErr(null);
    layout(n0, e0).then((p) => {
      if (!live) return;
      setPos(p);
      // vue initiale : la racine à gauche, au milieu, à une échelle lisible (le reste se parcourt à la souris)
      const root = n0.find((n) => n.data.isRoot);
      const r = root ? p[root.id] : undefined;
      const zoom = 0.62;
      setTimeout(() => rf.setViewport({ x: 28 - (r?.x ?? 0) * zoom, y: 320 - ((r?.y ?? 0) + 64) * zoom, zoom }, { duration: 200 }), 30);
    })
      .catch((e) => live && setErr(String(e)));
    return () => { live = false; };
  }, [n0, e0, rf]);

  const nodes = useMemo(() => n0.map((n) => ({ ...n, position: pos?.[n.id] ?? n.position, selected: n.id === selectedId })), [n0, pos, selectedId]);
  const edges: Edge<EdgeData>[] = useMemo(
    () => e0.filter((e) => showLoops || !e.data?.loopback || e.source === selectedId).map((e) => {
      const d = e.data!;
      const sel = e.source === selectedId;
      const color = d.kind === "success" ? "#63c3a0" : "#e0694b";
      return {
        ...e,
        label: pct(d.p),
        animated: false,
        style: { stroke: color, strokeWidth: 1 + Math.sqrt(d.p) * 3, opacity: sel ? 1 : d.loopback ? 0.35 : 0.7, strokeDasharray: d.loopback ? "3 5" : d.kind === "failure" ? "7 4" : undefined },
        labelStyle: { fill: "#ddd6c6", fontSize: 11 },
        labelBgStyle: { fill: "#0e1214", fillOpacity: 0.9 },
        labelBgPadding: [4, 2] as [number, number],
        markerEnd: { type: MarkerType.ArrowClosed, color, width: 14, height: 14 },
      };
    }),
    [e0, showLoops, selectedId],
  );

  if (err) return <div className="err" style={{ margin: 16 }}>Mise en page impossible : {err}</div>;
  return (
    <>
      {!pos && <div className="empty">Mise en page du graphe…</div>}
      <ReactFlow
        nodes={pos ? nodes : []} edges={pos ? edges : []} nodeTypes={nodeTypes} minZoom={0.1} maxZoom={1.6}
        nodesDraggable={false} nodesConnectable={false} elementsSelectable
        onNodeClick={(_, n) => onSelect(n.id)} onPaneClick={() => onSelect(null)} proOptions={{ hideAttribution: true }}
      >
        <Background variant={BackgroundVariant.Dots} gap={22} size={1} color="#1c272b" />
        <Controls showInteractive={false} position="bottom-left" />
        <MiniMap pannable zoomable position="bottom-right" nodeColor={(n) => (n.type === "goal" ? "#2f7a62" : "#3d4f55")} maskColor="rgba(14,18,20,0.7)" style={{ width: 150, height: 100 }} />
      </ReactFlow>
    </>
  );
}

export function PlanGraph({ plan }: { plan: CraftPlan }) {
  const [sel, setSel] = useState<string | null>(null);
  const [loops, setLoops] = useState(false);
  const total = Object.values(plan.nodes).filter((n) => n.kind === "action").length;
  const [cap, setCap] = useState(Math.min(30, total));
  useEffect(() => setSel(null), [plan]);
  const node = sel ? plan.nodes[sel] : null;
  return (
    <div className="graphwrap">
      <div className="graph-tools">
        <span className="legend">
          <span><i style={{ borderColor: "#63c3a0" }} />progrès</span>
          <span><i style={{ borderColor: "#e0694b", borderTopStyle: "dashed" }} />recul</span>
        </span>
        <label className="row small"><input type="checkbox" checked={loops} onChange={(e) => setLoops(e.target.checked)} /> retours en arrière</label>
        <label className="row small" title="Nombre d'étapes affichées, des plus fréquentes aux plus rares">
          étapes <input type="range" min={10} max={total} step={5} value={cap} onChange={(e) => setCap(+e.target.value)} style={{ width: 90 }} /> {Math.min(cap, total)}/{total}
        </label>
      </div>
      <ReactFlowProvider>
        <Inner plan={plan} cap={cap} onSelect={setSel} selectedId={sel} showLoops={loops} />
      </ReactFlowProvider>
      {node && node.kind === "action" && (
        <aside className="node-panel" aria-label="Détail de l'étape">
          <div className="row"><h3 className="hd grow">{node.action.label}</h3><button className="btn ghost sm" onClick={() => setSel(null)}>Fermer</button></div>
          <p className="muted small" style={{ margin: "4px 0 10px" }}>
            Coût restant espéré {cost(node.costToGo)} · {num(node.expectedVisits, 2)} passage(s) par craft
            {node.repeat && ` · ${num(node.repeat.expectedAttempts, 1)} essais avant de changer d'état (9 fois sur 10 : ${Math.ceil(node.repeat.p90Attempts)})`}
          </p>
          <div className="hd small" style={{ marginBottom: 4 }}>Selon le tirage</div>
          {node.branches.map((b) => (
            <div key={b.id} className={`br ${b.kind}`}>
              <b>{pct(b.probability)}</b>
              <span>{b.label}{b.loopback && <span className="faint"> ↺</span>}{b.extraCost > 0 && <span className="faint"> (−{cost(b.extraCost)})</span>}</span>
            </div>
          ))}
          {node.mergedMinorCount > 0 && <p className="faint small" style={{ marginTop: 8 }}>+ {node.mergedMinorCount} issue(s) rare(s) : {pct(node.mergedMinorProbability)} au total</p>}
        </aside>
      )}
    </div>
  );
}
