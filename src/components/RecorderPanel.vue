<script setup lang="ts">
import { ref } from "vue";
import { recorderCommit, recorderStart, recorderStop, type RecordedStep } from "../api/maa";

const props = defineProps<{ resourceDir: string }>();
const emit = defineEmits<{
  (event: "log", message: string): void;
  (event: "committed"): void;
}>();

const mode = ref("smart");
const steps = ref<RecordedStep[]>([]);
const busy = ref(false);

/** 统一执行并记录日志，避免每个按钮重复 try-catch */
async function run(label: string, action: () => Promise<unknown>) {
  busy.value = true;
  try {
    const result = await action();
    if (typeof result === "string") {
      emit("log", `${label}：${result}`);
    }
  } catch (error) {
    emit("log", `${label}失败：${String(error)}`);
  } finally {
    busy.value = false;
  }
}

async function onStart() {
  steps.value = [];
  await run("开始录制", () => recorderStart(mode.value, props.resourceDir));
}

async function onStop() {
  const result = await recorderStop();
  steps.value = result;
  emit("log", `录制结束，共 ${result.length} 个操作`);
}

async function onCommit() {
  await run("生成 pipeline", () => recorderCommit());
  emit("committed");
}

function describe(step: RecordedStep): string {
  switch (step.kind) {
    case "Click":
      return step.template
        ? `点击 (${step.x}, ${step.y}) → 模板 ${step.template}`
        : `点击 (${step.x}, ${step.y})（坐标模式）`;
    case "Swipe":
      return `滑动 (${step.x}, ${step.y}) → (${step.end_x}, ${step.end_y})`;
    case "Text":
      return `输入字符 "${step.text}"`;
    case "Key":
      return `按键 ${step.key}`;
    default:
      return step.kind;
  }
}
</script>

<template>
  <section class="panel">
    <h2>录制引擎</h2>

    <div class="row">
      <select v-model="mode">
        <option value="smart">智能录制（识图，推荐）</option>
        <option value="coordinate">坐标录制（兜底，抗变性差）</option>
      </select>
      <button :disabled="busy" @click="onStart">开始录制</button>
      <button :disabled="busy" @click="onStop">停止</button>
      <button :disabled="busy || steps.length === 0" @click="onCommit">
        生成 Pipeline
      </button>
    </div>

    <p class="tip">
      智能录制会截取每次点击周围的区域存为模板图，并生成 TemplateMatch 识别节点；
      连续输入的字符会合并为一个 InputText 节点。模板保存在
      <code>{{ resourceDir }}/image/</code>
    </p>

    <h3>步骤预览（{{ steps.length }}）</h3>
    <ol class="steps">
      <li v-for="(step, index) in steps" :key="index">{{ describe(step) }}</li>
      <li v-if="steps.length === 0" class="muted">暂无步骤，点击「开始录制」</li>
    </ol>
  </section>
</template>

<style scoped>
.panel {
  padding: 16px;
  border: 1px solid #e5e7eb;
  border-radius: 10px;
  background: #fff;
}
h2 {
  margin: 0 0 12px;
  font-size: 16px;
}
h3 {
  margin: 16px 0 8px;
  font-size: 14px;
}
.row {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}
select {
  padding: 7px 10px;
  border: 1px solid #d1d5db;
  border-radius: 6px;
}
button {
  padding: 7px 14px;
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
.tip {
  margin: 10px 0 0;
  font-size: 12px;
  color: #6b7280;
  line-height: 1.7;
}
.steps {
  margin: 0;
  padding-left: 20px;
  font-size: 13px;
  line-height: 1.8;
}
.muted {
  color: #9ca3af;
  list-style: none;
  margin-left: -20px;
}
</style>
