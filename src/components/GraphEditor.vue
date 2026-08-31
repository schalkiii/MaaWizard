<script setup lang="ts">
import {
  VueFlow,
  type Connection,
  type Edge,
  type GraphNode,
  type VueFlowStore,
} from "@vue-flow/core";
import { Background } from "@vue-flow/background";
import { Controls } from "@vue-flow/controls";
import { MiniMap } from "@vue-flow/minimap";
import "@vue-flow/core/dist/style.css";
import "@vue-flow/core/dist/theme-default.css";
import "@vue-flow/controls/dist/style.css";
import "@vue-flow/minimap/dist/style.css";
import { computed, ref } from "vue";

import type { PipelineDocument, ValidationIssue } from "../api/maa";
import JumpBackNodeView from "./JumpBackNodeView.vue";
import PipelineNodeView from "./PipelineNodeView.vue";
import {
  buildEdges,
  buildNodes,
  JUMPBACK_ID,
  recognitionColor,
  type EdgeKind,
  type NodePosition,
  type PipelineNodeViewData,
} from "./graph";

const props = withDefaults(
  defineProps<{
    document: PipelineDocument;
    /** 校验问题，用于在节点上打角标 */
    issues?: ValidationIssue[];
    /** 用户手动摆放过的位置，优先于自动布局 */
    positions?: Record<string, NodePosition>;
  }>(),
  { issues: () => [], positions: () => ({}) },
);

const emit = defineEmits<{
  (event: "select", name: string): void;
  (event: "connect", payload: { source: string; target: string; kind: EdgeKind }): void;
  (event: "disconnect", payload: { source: string; target: string; kind: EdgeKind }): void;
  (event: "move", payload: { name: string; position: NodePosition }): void;
}>();

const nodeTypes = {
  pipeline: PipelineNodeView,
  jumpback: JumpBackNodeView,
};

const nodes = computed(() => buildNodes(props.document, props.positions, props.issues));
const edges = computed(() => buildEdges(props.document));

const selectedEdgeId = ref<string | null>(null);
let flow: VueFlowStore | null = null;

function onPaneReady(instance: VueFlowStore) {
  flow = instance;
}

function onNodeClick(event: { node: { id: string } }) {
  selectedEdgeId.value = null;
  // [JumpBack] 是合成标记节点，不可选中编辑
  if (event.node.id !== JUMPBACK_ID) {
    emit("select", event.node.id);
  }
}

/** 从右侧出口拖出的是 next，从下方出口拖出的是 on_error */
function onConnect(connection: Connection) {
  if (!connection.source || !connection.target) {
    return;
  }
  emit("connect", {
    source: connection.source,
    target: connection.target,
    kind: connection.sourceHandle === "on_error" ? "on_error" : "next",
  });
}

function onEdgeClick(event: { edge: Edge }) {
  selectedEdgeId.value = event.edge.id;
}

function removeSelectedEdge() {
  const target = edges.value.find((edge) => edge.id === selectedEdgeId.value);
  if (!target) {
    return;
  }
  emit("disconnect", {
    source: target.source,
    target: target.target,
    kind: target.kind,
  });
  selectedEdgeId.value = null;
}

function onNodeDragStop(event: { node: { id: string; position: NodePosition } }) {
  if (event.node.id === JUMPBACK_ID) {
    return;
  }
  emit("move", { name: event.node.id, position: event.node.position });
}

function fitView() {
  flow?.fitView({ padding: 0.2 });
}

function minimapColor(node: GraphNode) {
  return recognitionColor((node.data as PipelineNodeViewData).recognition);
}

defineExpose({ fitView });
</script>

<template>
  <div class="editor">
    <div class="toolbar">
      <span class="stat">{{ nodes.length }} 节点 · {{ edges.length }} 连线</span>
      <span class="legend">
        <i class="line next" />next
        <i class="line error" />on_error
      </span>
      <span class="spacer" />
      <button type="button" class="tool" :disabled="!selectedEdgeId" @click="removeSelectedEdge">
        删除选中连线
      </button>
      <button type="button" class="tool" @click="fitView">适应视图</button>
    </div>

    <div class="canvas">
      <VueFlow
        :nodes="nodes"
        :edges="edges"
        :node-types="nodeTypes"
        :fit-view-on-init="true"
        :min-zoom="0.2"
        :max-zoom="2"
        @pane-ready="onPaneReady"
        @connect="onConnect"
        @edge-click="onEdgeClick"
        @node-click="onNodeClick"
        @node-drag-stop="onNodeDragStop"
      >
        <Background :gap="18" pattern-color="#d1d5db" />
        <Controls />
        <MiniMap :node-color="minimapColor" pannable zoomable />
      </VueFlow>
    </div>

    <p v-if="Object.keys(document).length === 0" class="empty">
      尚未加载节点。可打开资源包、录制操作，或点击「新建节点」。
    </p>
    <p v-else class="tip">
      拖拽节点<strong>右侧</strong>圆点连 next、<strong>下方</strong>圆点连 on_error；点击连线可选中并删除。
    </p>
  </div>
</template>

<style scoped>
.editor {
  margin-top: 10px;
}
.toolbar {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 8px;
  font-size: 12px;
  color: #4b5563;
}
.spacer {
  flex: 1;
}
.stat {
  font-weight: 600;
}
.legend {
  display: flex;
  align-items: center;
  gap: 6px;
  color: #6b7280;
}
.line {
  display: inline-block;
  width: 18px;
  height: 0;
  border-top: 2px solid #2563eb;
}
.line.error {
  border-top: 2px dashed #dc2626;
}
.tool {
  padding: 4px 10px;
  border: 1px solid #d1d5db;
  border-radius: 6px;
  background: #fff;
  color: #374151;
  font-size: 12px;
  cursor: pointer;
}
.tool:hover:not(:disabled) {
  border-color: #2563eb;
  color: #2563eb;
}
.tool:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}
.canvas {
  height: 460px;
  border: 1px solid #e5e7eb;
  border-radius: 10px;
  overflow: hidden;
  background: #fafafa;
}
.empty,
.tip {
  margin: 8px 0 0;
  font-size: 12px;
  color: #6b7280;
}
.empty {
  padding: 24px;
  border: 1px dashed #d1d5db;
  border-radius: 8px;
  text-align: center;
}
</style>
