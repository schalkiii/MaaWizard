/**
 * 图编辑器的纯数据映射层：PipelineDocument ⇄ Vue Flow 的节点与连线。
 *
 * 与渲染解耦，因此可以在不挂载组件的情况下完整测试（见 graph.spec.ts）。
 */

import type { PipelineDocument, PipelineNodeData, ValidationIssue } from "../api/maa";

export interface NodePosition {
  x: number;
  y: number;
}

/** [JumpBack] 在画布上用一个合成节点表示，它不是真实节点，不可选中编辑 */
export const JUMPBACK_ID = "__jumpback__";

/** 连线语义：识别成功走 next，超时/失败走 on_error */
export type EdgeKind = "next" | "on_error";

export interface PipelineNodeViewData {
  name: string;
  recognition: string;
  action: string;
  /** 该节点的校验错误 / 提示数量，用于在节点上打角标 */
  errors: number;
  warnings: number;
  isEntry: boolean;
}

export interface PipelineGraphNode {
  id: string;
  type: "pipeline" | "jumpback";
  position: NodePosition;
  data: PipelineNodeViewData;
}

export interface PipelineGraphEdge {
  id: string;
  source: string;
  target: string;
  label: string;
  kind: EdgeKind;
  animated: boolean;
  style: Record<string, string>;
}

/** 各识别类型的主题色，用于在画布上快速区分节点性质 */
const RECOGNITION_COLORS: Record<string, string> = {
  DirectHit: "#9ca3af",
  TemplateMatch: "#2563eb",
  FeatureMatch: "#7c3aed",
  ColorMatch: "#0891b2",
  OCR: "#059669",
  NeuralNetworkClassify: "#db2777",
  NeuralNetworkDetect: "#c026d3",
  And: "#ea580c",
  Or: "#ca8a04",
  Custom: "#475569",
};

export function recognitionColor(kind: string): string {
  return RECOGNITION_COLORS[kind] ?? "#64748b";
}

/** 兼容 V1（字符串）与 V2（{type,param}）两种写法 */
export function describeSpec(spec: unknown): string {
  if (typeof spec === "string") {
    return spec;
  }
  if (spec && typeof spec === "object" && "type" in spec) {
    return String((spec as Record<string, unknown>).type);
  }
  return "-";
}

/** 取出 next/on_error 条目指向的节点名（对象形式取 name 字段） */
export function entryTarget(entry: unknown): string | null {
  if (typeof entry === "string") {
    return entry;
  }
  if (entry && typeof entry === "object" && "name" in entry) {
    return String((entry as Record<string, unknown>).name);
  }
  return null;
}

/** 按 next/on_error 统计每个节点被指向的次数 */
function countIncoming(document: PipelineDocument): Map<string, number> {
  const counts = new Map<string, number>();
  for (const name of Object.keys(document)) {
    counts.set(name, 0);
  }
  for (const name of Object.keys(document)) {
    const node: PipelineNodeData = document[name];
    for (const entry of [...(node.next ?? []), ...(node.on_error ?? [])]) {
      const target = entryTarget(entry);
      if (target && counts.has(target)) {
        counts.set(target, (counts.get(target) ?? 0) + 1);
      }
    }
  }
  return counts;
}

/**
 * 依据 next 链做分层布局：入度为 0 的节点作为根，
 * 按 BFS 深度分列、同层内顺序分行。环内节点统一放到最后一列。
 */
export function buildLayout(document: PipelineDocument): Map<string, NodePosition> {
  const names = Object.keys(document);
  const incoming = countIncoming(document);

  const adjacency = new Map<string, string[]>(names.map((name) => [name, []]));
  for (const name of names) {
    const node: PipelineNodeData = document[name];
    for (const entry of node.next ?? []) {
      const target = entryTarget(entry);
      if (target && incoming.has(target) && target !== name) {
        adjacency.get(name)?.push(target);
      }
    }
  }

  const roots = names.filter((name) => (incoming.get(name) ?? 0) === 0);
  const start = roots.length > 0 ? roots : names.slice(0, 1);

  const depth = new Map<string, number>();
  const visited = new Set<string>();
  const queue: Array<[string, number]> = start.map((name) => [name, 0]);

  while (queue.length > 0) {
    const [name, level] = queue.shift()!;
    if (visited.has(name)) {
      continue;
    }
    visited.add(name);
    depth.set(name, Math.min(level, depth.get(name) ?? Number.POSITIVE_INFINITY));
    for (const next of adjacency.get(name) ?? []) {
      if (!visited.has(next)) {
        queue.push([next, level + 1]);
      }
    }
  }

  let maxDepth = depth.size > 0 ? Math.max(...depth.values()) : 0;
  for (const name of names) {
    if (!depth.has(name)) {
      maxDepth += 1;
      depth.set(name, maxDepth);
    }
  }

  const byDepth = new Map<number, string[]>();
  for (const name of names) {
    const level = depth.get(name) ?? 0;
    const group = byDepth.get(level) ?? [];
    group.push(name);
    byDepth.set(level, group);
  }

  const positions = new Map<string, NodePosition>();
  for (const [level, group] of byDepth) {
    group.forEach((name, index) => {
      positions.set(name, { x: level * 300, y: index * 130 });
    });
  }
  return positions;
}

/** 入口节点：没有任何入边的第一个节点 */
export function entryNode(document: PipelineDocument): string | null {
  const incoming = countIncoming(document);
  return Object.keys(document).find((name) => (incoming.get(name) ?? 0) === 0) ?? null;
}

/** 文档里是否存在指向 [JumpBack] 的跳转 */
export function hasJumpBack(document: PipelineDocument): boolean {
  return Object.values(document).some((node) =>
    [...(node.next ?? []), ...(node.on_error ?? [])].some(
      (entry) => entryTarget(entry) === "[JumpBack]",
    ),
  );
}

/** 把校验问题汇总成「节点 → 错误/提示数」 */
export function countIssues(
  issues: ValidationIssue[],
): Map<string, { errors: number; warnings: number }> {
  const counts = new Map<string, { errors: number; warnings: number }>();
  for (const issue of issues) {
    if (!issue.node) {
      continue;
    }
    const item = counts.get(issue.node) ?? { errors: 0, warnings: 0 };
    if (issue.level === "error") {
      item.errors += 1;
    } else {
      item.warnings += 1;
    }
    counts.set(issue.node, item);
  }
  return counts;
}

/**
 * 构建画布节点。已保存过位置的节点沿用用户摆放的位置，
 * 其余按自动布局排列，这样手动调整不会因为刷新文档而丢失。
 */
export function buildNodes(
  document: PipelineDocument,
  savedPositions: Record<string, NodePosition> = {},
  issues: ValidationIssue[] = [],
): PipelineGraphNode[] {
  const layout = buildLayout(document);
  const counts = countIssues(issues);
  const entry = entryNode(document);

  const nodes: PipelineGraphNode[] = Object.keys(document).map((name) => {
    const node: PipelineNodeData = document[name];
    const count = counts.get(name);
    return {
      id: name,
      type: "pipeline",
      position: savedPositions[name] ?? layout.get(name) ?? { x: 0, y: 0 },
      data: {
        name,
        recognition: describeSpec(node.recognition),
        action: describeSpec(node.action),
        errors: count?.errors ?? 0,
        warnings: count?.warnings ?? 0,
        isEntry: name === entry,
      },
    };
  });

  if (hasJumpBack(document)) {
    nodes.push({
      id: JUMPBACK_ID,
      type: "jumpback",
      position: savedPositions[JUMPBACK_ID] ?? { x: 0, y: -170 },
      data: {
        name: "[JumpBack]",
        recognition: "回跳点",
        action: "-",
        errors: 0,
        warnings: 0,
        isEntry: false,
      },
    });
  }

  return nodes;
}

/** 构建画布连线：next 为蓝色实线，on_error 为红色虚线 */
export function buildEdges(document: PipelineDocument): PipelineGraphEdge[] {
  const edges: PipelineGraphEdge[] = [];

  for (const name of Object.keys(document)) {
    const node: PipelineNodeData = document[name];

    for (const entry of node.next ?? []) {
      const target = entryTarget(entry);
      if (!target) {
        continue;
      }
      edges.push({
        id: `${name}->${target}:next`,
        source: name,
        target: target === "[JumpBack]" ? JUMPBACK_ID : target,
        label: "next",
        kind: "next",
        animated: true,
        style: { stroke: "#2563eb", strokeWidth: "2" },
      });
    }

    for (const entry of node.on_error ?? []) {
      const target = entryTarget(entry);
      if (!target) {
        continue;
      }
      edges.push({
        id: `${name}->${target}:on_error`,
        source: name,
        target: target === "[JumpBack]" ? JUMPBACK_ID : target,
        label: "on_error",
        kind: "on_error",
        animated: false,
        style: { stroke: "#dc2626", strokeWidth: "2", strokeDasharray: "5 5" },
      });
    }
  }

  return edges;
}
