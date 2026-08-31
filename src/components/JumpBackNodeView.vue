<script setup lang="ts">
import { Handle, Position, type NodeProps } from "@vue-flow/core";

defineProps<NodeProps>();
</script>

<template>
  <div class="jumpback" title="[JumpBack] 是框架约定的回跳标记，不是真实节点">
    <Handle id="in" type="target" :position="Position.Left" class="handle" />
    <span>↺ [JumpBack]</span>
  </div>
</template>

<style scoped>
.jumpback {
  padding: 5px 12px;
  border: 1px dashed #f59e0b;
  border-radius: 999px;
  background: #fffbeb;
  color: #92400e;
  font-size: 12px;
}
.handle {
  width: 9px;
  height: 9px;
  border: 2px solid #fff;
  background: #f59e0b;
}
</style>
