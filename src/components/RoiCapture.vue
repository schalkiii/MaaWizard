<script setup lang="ts">
import { ref } from "vue";
import { convertFileSrc } from "@tauri-apps/api/core";
import { captureGrabTemplate, captureScreenshot } from "../api/maa";

const props = defineProps<{ resourceDir: string }>();
const emit = defineEmits<{
  (event: "log", message: string): void;
  (event: "apply", file: string): void;
}>();

const imageSrc = ref("");
const busy = ref(false);
const dragging = ref(false);
const start = ref({ x: 0, y: 0 });
const rect = ref({ x: 0, y: 0, w: 0, h: 0 });
const imageElement = ref<HTMLImageElement | null>(null);

async function onCapture() {
  busy.value = true;
  try {
    // 以 . 开头的文件不会被 MaaFramework 当作资源读取
    const path = await captureScreenshot(`${props.resourceDir}/.screenshot.png`);
    imageSrc.value = convertFileSrc(path);
    emit("log", `已截图：${path}`);
  } catch (error) {
    emit("log", `截图失败：${String(error)}`);
  } finally {
    busy.value = false;
  }
}

function relativePosition(event: MouseEvent) {
  const bounds = (event.currentTarget as HTMLElement).getBoundingClientRect();
  return { x: event.clientX - bounds.left, y: event.clientY - bounds.top };
}

function onMouseDown(event: MouseEvent) {
  const position = relativePosition(event);
  start.value = position;
  rect.value = { x: position.x, y: position.y, w: 0, h: 0 };
  dragging.value = true;
}

function onMouseMove(event: MouseEvent) {
  if (!dragging.value) {
    return;
  }
  const current = relativePosition(event);
  rect.value = {
    x: Math.min(start.value.x, current.x),
    y: Math.min(start.value.y, current.y),
    w: Math.abs(current.x - start.value.x),
    h: Math.abs(current.y - start.value.y),
  };
}

function onMouseUp() {
  dragging.value = false;
}

async function onGrab() {
  if (rect.value.w < 4 || rect.value.h < 4) {
    emit("log", "请先在截图上拖拽框选一块区域");
    return;
  }

  // 截图按 CSS 宽度显示，需换算回原始像素后再交给后端裁剪
  const image = imageElement.value;
  const scale =
    image && image.clientWidth > 0 ? image.naturalWidth / image.clientWidth : 1;

  busy.value = true;
  try {
    const result = await captureGrabTemplate(
      Math.round(rect.value.x * scale),
      Math.round(rect.value.y * scale),
      Math.round(rect.value.w * scale),
      Math.round(rect.value.h * scale),
      props.resourceDir,
    );
    emit("log", `已抓取模板 ${result.file}，roi=[${result.roi.join(", ")}]`);
    emit("apply", result.file);
  } catch (error) {
    emit("log", `抓取模板失败：${String(error)}`);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <section class="roi">
    <h3>ROI 框选与模板抓取</h3>
    <div class="row">
      <button :disabled="busy" @click="onCapture">截取屏幕</button>
      <button :disabled="busy || !imageSrc" @click="onGrab">抓取选中区域为模板</button>
      <span v-if="rect.w > 0" class="size">
        {{ Math.round(rect.w) }}×{{ Math.round(rect.h) }}
      </span>
    </div>

    <div
      v-if="imageSrc"
      class="stage"
      @mousedown="onMouseDown"
      @mousemove="onMouseMove"
      @mouseup="onMouseUp"
      @mouseleave="onMouseUp"
    >
      <img ref="imageElement" :src="imageSrc" alt="屏幕截图" draggable="false" />
      <div
        v-if="rect.w > 0"
        class="selection"
        :style="{
          left: `${rect.x}px`,
          top: `${rect.y}px`,
          width: `${rect.w}px`,
          height: `${rect.h}px`,
        }"
      />
    </div>
    <p v-else class="hint">点击「截取屏幕」后，在图上拖拽即可框选模板区域。</p>
  </section>
</template>

<style scoped>
.roi {
  margin-top: 14px;
  padding: 12px;
  border: 1px solid #e5e7eb;
  border-radius: 8px;
  background: #fafafa;
}
h3 {
  margin: 0 0 10px;
  font-size: 14px;
}
.row {
  display: flex;
  gap: 8px;
  align-items: center;
  margin-bottom: 10px;
}
button {
  padding: 6px 12px;
  border: 1px solid #2563eb;
  border-radius: 6px;
  background: #2563eb;
  color: #fff;
  cursor: pointer;
  font-size: 13px;
}
button:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.size {
  font-size: 12px;
  color: #6b7280;
}
.stage {
  position: relative;
  display: inline-block;
  max-width: 100%;
  cursor: crosshair;
  user-select: none;
}
.stage img {
  max-width: 100%;
  display: block;
  border-radius: 6px;
}
.selection {
  position: absolute;
  border: 2px solid #2563eb;
  background: rgba(37, 99, 235, 0.18);
  pointer-events: none;
}
.hint {
  margin: 0;
  font-size: 12px;
  color: #9ca3af;
}
</style>
