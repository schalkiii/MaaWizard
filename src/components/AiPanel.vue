<script setup lang="ts">
import { ref } from "vue";
import { aiDetect, aiRun, type AiEnvironment } from "../api/maa";

const emit = defineEmits<{ (event: "log", message: string): void }>();

const environment = ref<AiEnvironment | null>(null);
const program = ref("uvx");
const args = ref("maafw-cli --help");
const output = ref("");
const busy = ref(false);

async function onDetect() {
  busy.value = true;
  try {
    environment.value = await aiDetect();
    emit("log", `AI 环境探测：${environment.value.suggestion}`);
  } catch (error) {
    emit("log", `环境探测失败：${String(error)}`);
  } finally {
    busy.value = false;
  }
}

/**
 * 运行外部 AI 工具。按 ADR 0004，Python 运行时不打包进安装包，
 * 缺失时优雅降级：这里只提示，不阻断其它功能。
 */
async function onRun() {
  busy.value = true;
  try {
    const parsedArgs = args.value.split(" ").filter((item) => item.trim() !== "");
    output.value = await aiRun(program.value, parsedArgs);
    emit("log", `已执行 ${program.value} ${parsedArgs.join(" ")}`);
  } catch (error) {
    output.value = String(error);
    emit("log", `执行失败：${String(error)}`);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <section class="panel">
    <h2>AI 增强</h2>

    <div class="row">
      <button :disabled="busy" @click="onDetect">探测环境</button>
      <span v-if="environment" :class="environment.usable ? 'ok' : 'bad'">
        {{ environment.usable ? "可用" : "不可用" }}
      </span>
    </div>

    <p v-if="environment" class="tip">{{ environment.suggestion }}</p>
    <ul v-if="environment" class="env">
      <li>python：{{ environment.python ?? "未检测到" }}</li>
      <li>uv：{{ environment.uv ?? "未检测到" }}</li>
      <li>uvx：{{ environment.uvx ?? "未检测到" }}</li>
    </ul>

    <h3>运行 AI 工具</h3>
    <div class="row">
      <input v-model="program" placeholder="程序（如 uvx / maafw-cli）" />
      <input v-model="args" placeholder="参数（空格分隔）" />
      <button :disabled="busy" @click="onRun">执行</button>
    </div>
    <p class="tip">
      例如用 <code>uvx maafw-cli device</code> 查看设备、<code>uvx maafw-cli ocr</code>
      观察屏幕。生成的 pipeline 可被本工具加载运行，形成
      「生成 → 运行 → 观测 → 分析 → 修正」的迭代闭环。
    </p>

    <pre v-if="output" class="output">{{ output }}</pre>
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
  align-items: center;
  margin-bottom: 8px;
  flex-wrap: wrap;
}
input {
  flex: 1;
  min-width: 160px;
  padding: 7px 10px;
  border: 1px solid #d1d5db;
  border-radius: 6px;
  font-size: 13px;
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
.ok {
  color: #059669;
  font-size: 13px;
}
.bad {
  color: #dc2626;
  font-size: 13px;
}
.tip {
  font-size: 12px;
  color: #6b7280;
  line-height: 1.7;
}
.env {
  font-size: 12px;
  color: #374151;
  margin: 6px 0;
  padding-left: 18px;
}
.output {
  background: #0f172a;
  color: #e2e8f0;
  padding: 10px;
  border-radius: 6px;
  font-size: 12px;
  max-height: 220px;
  overflow: auto;
  white-space: pre-wrap;
}
</style>
