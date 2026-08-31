<script setup lang="ts">
import ControllerPanel from "./ControllerPanel.vue";

defineEmits<{
  (event: "log", message: string): void;
  (event: "controller", type: string): void;
}>();
</script>

<template>
  <section class="card">
    <h2>设备管理</h2>
    <ControllerPanel @log="$emit('log', $event)" @controller="$emit('controller', $event)" />
  </section>
</template>

<style scoped>
.card {
  padding: 16px;
  border: 1px solid #e5e7eb;
  border-radius: 10px;
  background: #fff;
  margin-bottom: 14px;
}
h2 {
  margin: 0 0 12px;
  font-size: 16px;
}
</style>
