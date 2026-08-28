<script setup lang="ts">
import { computed } from "vue";
import { VueFlow, type Edge } from "@vue-flow/core";
import "@vue-flow/core/dist/style.css";
import "@vue-flow/core/dist/theme-default.css";
import type { PipelineDocument } from "../api/maa";

const props = defineProps<{ document: PipelineDocument }>();
const emit = defineEmits<{ (event: "select", name: string): void }>();

/** 取出 next/on_error 条目指向的节点名（对象形式取 name 字段） */
function entryTarget(entry: unknown): string | null {
  if (typeof entry === "string") {
    return entry;
  }
  if (entry && typeof entry === "object" && "name" in entry) {
    return String((entry as Record<string, unknown>).name);
  }
  return null;
}

/**
 * 依据 next 链做简单分层布局：入度为 0 的节点作为根，
 * 按 BFS 深度分列、同层内顺序分行。
 */
function buildLayout(document: PipelineDocument) {
  const names = Object.keys(document);
  const indegree = new Map<string, number>(names.map((name) => [name, 0]));
  const adjacency = new Map<string, string[]>(names.map((name) => [name, []]));

  for (const name of names) {
    const node = document[name];
    const entries = [...(node.next ?? []), ...(node.on_error ?? [])];
    for (const entry of entries) {
      const target = entryTarget(entry);
      if (target && indegree.has(target)) {
        indegree.set(target, (indegree.get(target) ?? 0) + 1);
        adjacency.get(name)?.push(target);
      }
    }
  }

  const roots = names.filter((name) => (indegree.get(name) ?? 0) === 0);
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

  // 环内等无法从根到达的节点，统一放到最后
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

  const positions = new Map<string, { x: number; y: number }>();
  for (const [level, group] of byDepth) {
    group.forEach((name, index) => {
      positions.set(name, { x: level * 280, y: index * 110 });
    });
  }
  return positions;
}

const nodes = computed(() => {
  const document = props.document;
  const positions = buildLayout(document);
  const result = Object.keys(document).map((name) => {
    const node = document[name];
    const recognition = describeSpec(node.recognition);
    const action = describeSpec(node.action);
    return {
      id: name,
      position: positions.get(name) ?? { x: 0, y: 0 },
      data: { label: `${name}\n${recognition} → ${action}` },
      // 记录原始节点，供父组件在选中时直接读取
      raw: node,
    };
  });

  // [JumpBack] 是 next 列表中的特殊标记，用合成节点可视化出来
  const hasJumpBack = Object.values(document).some((node) =>
    [...(node.next ?? []), ...(node.on_error ?? [])].some(
      (entry) => entryTarget(entry) === "[JumpBack]",
    ),
  );
  if (hasJumpBack) {
    result.push({
      id: "__jumpback__",
      position: { x: 0, y: -140 },
      data: { label: "[JumpBack]\n回跳点" },
      raw: {},
    });
  }
  return result;
});

const edges = computed(() => {
  const document = props.document;
  const result: Edge[] = [];

  for (const name of Object.keys(document)) {
    const node = document[name];
    for (const entry of node.next ?? []) {
      const target = entryTarget(entry);
      if (!target) continue;
      result.push({
        id: `${name}->${target}`,
        source: name,
        target: target === "[JumpBack]" ? "__jumpback__" : target,
        animated: true,
        style: { stroke: "#2563eb" },
        label: "next",
      });
    }
    for (const entry of node.on_error ?? []) {
      const target = entryTarget(entry);
      if (!target) continue;
      result.push({
        id: `${name}-err->${target}`,
        source: name,
        target: target === "[JumpBack]" ? "__jumpback__" : target,
        style: { stroke: "#dc2626", strokeDasharray: "5 5" },
        label: "on_error",
      });
    }
  }
  return result;
});

/** 兼容 V1（字符串）与 V2（{type,param}）两种写法 */
function describeSpec(spec: unknown): string {
  if (typeof spec === "string") {
    return spec;
  }
  if (spec && typeof spec === "object" && "type" in spec) {
    return String((spec as Record<string, unknown>).type);
  }
  return "-";
}

function onNodeClick(event: { node: { id: string } }) {
  if (event.node.id !== "__jumpback__") {
    emit("select", event.node.id);
  }
}
</script>

<template>
  <div class="editor">
    <VueFlow
      :nodes="nodes"
      :edges="edges"
      :fit-view-on-init="true"
      @node-click="onNodeClick"
    />
    <p v-if="Object.keys(document).length === 0" class="empty">
      尚未加载节点。可打开资源包、录制操作，或点击「新建节点」。
    </p>
  </div>
</template>

<style scoped>
.editor {
  height: 420px;
  border: 1px solid #e5e7eb;
  border-radius: 8px;
  position: relative;
  background: #fafafa;
}
.empty {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  margin: 0;
  color: #9ca3af;
  pointer-events: none;
  font-size: 13px;
}
</style>
