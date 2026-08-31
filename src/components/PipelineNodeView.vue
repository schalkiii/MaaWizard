<script setup lang="ts">
import { Handle, Position, type NodeProps } from "@vue-flow/core";
import { computed } from "vue";

import { recognitionColor, type PipelineNodeViewData } from "./graph";

/**
 * 画布上的单个 pipeline 节点。
 * 左侧竖条按识别类型着色，右上角显示校验角标，
 * 右侧出口连 next、下方出口连 on_error。
 *
 * 使用 Vue Flow 的 NodeProps 作为 props 类型，才能满足 nodeTypes 的类型要求。
 */
const props = defineProps<NodeProps<PipelineNodeViewData>>();

const accent = computed(() => recognitionColor(props.data.recognition));
</script>

<template>
  <div
    class="node"
    :class="{ selected, 'has-error': data.errors > 0 }"
    :style="{ borderLeftColor: accent }"
  >
    <Handle id="in" type="target" :position="Position.Left" class="handle" />

    <div class="head">
      <span class="name" :title="data.name">{{ data.name }}</span>
      <span v-if="data.isEntry" class="tag entry">入口</span>
      <span
        v-if="data.errors > 0"
        class="tag error"
        :title="`${data.errors} 个校验错误`"
      >
        ✕{{ data.errors }}
      </span>
      <span
        v-else-if="data.warnings > 0"
        class="tag warn"
        :title="`${data.warnings} 条提示`"
      >
        !{{ data.warnings }}
      </span>
    </div>

    <div class="body">
      <span class="badge reco" :style="{ background: accent }">{{ data.recognition }}</span>
      <span class="arrow">→</span>
      <span class="badge action">{{ data.action }}</span>
    </div>

    <Handle id="next" type="source" :position="Position.Right" class="handle next" />
    <Handle id="on_error" type="source" :position="Position.Bottom" class="handle error" />
  </div>
</template>

<style scoped>
.node {
  width: 190px;
  padding: 8px 10px;
  border: 1px solid #e5e7eb;
  border-left: 5px solid #2563eb;
  border-radius: 8px;
  background: #fff;
  box-shadow: 0 1px 3px rgb(15 23 42 / 12%);
  font-size: 12px;
  color: #1f2933;
}
.node.selected {
  border-color: #2563eb;
  box-shadow: 0 0 0 2px rgb(37 99 235 / 35%);
}
.node.has-error {
  border-color: #fca5a5;
  background: #fff5f5;
}
.head {
  display: flex;
  align-items: center;
  gap: 5px;
  margin-bottom: 6px;
}
.name {
  flex: 1;
  overflow: hidden;
  font-weight: 600;
  font-size: 13px;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.tag {
  padding: 1px 5px;
  border-radius: 8px;
  font-size: 10px;
  line-height: 1.5;
}
.tag.entry {
  background: #dbeafe;
  color: #1d4ed8;
}
.tag.error {
  background: #fee2e2;
  color: #b91c1c;
}
.tag.warn {
  background: #fef3c7;
  color: #92400e;
}
.body {
  display: flex;
  align-items: center;
  gap: 6px;
}
.badge {
  padding: 2px 6px;
  border-radius: 4px;
  font-size: 11px;
  color: #fff;
  white-space: nowrap;
}
.badge.action {
  background: #475569;
}
.arrow {
  color: #9ca3af;
}
.handle {
  width: 10px;
  height: 10px;
  border: 2px solid #fff;
  background: #64748b;
}
.handle.next {
  background: #2563eb;
}
.handle.error {
  background: #dc2626;
}
</style>
