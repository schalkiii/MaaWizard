<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import AiPanel from "./components/AiPanel.vue";
import DevicePanel from "./components/DevicePanel.vue";
import GraphEditor from "./components/GraphEditor.vue";
import NodeInspector from "./components/NodeInspector.vue";
import RecorderPanel from "./components/RecorderPanel.vue";
import RoiCapture from "./components/RoiCapture.vue";
import {
  loadLibrary,
  loadResource,
  onMaaEvent,
  pipelineAddNode,
  pipelineDeleteNode,
  pipelineGet,
  pipelineOpen,
  pipelineSave,
  pipelineUpdateNode,
  runTask,
  runtimeStatus,
  stopTask,
  type PipelineDocument,
  type PipelineNodeData,
} from "./api/maa";

type TabKey = "run" | "editor" | "record" | "device" | "ai";

const tabs: Array<{ key: TabKey; label: string }> = [
  { key: "run", label: "运行" },
  { key: "editor", label: "图编辑器" },
  { key: "record", label: "录制" },
  { key: "device", label: "设备" },
  { key: "ai", label: "AI" },
];

const tab = ref<TabKey>("run");
const busy = ref(false);

// 运行时
const dllPath = ref("maa-sdk/bin/MaaFramework.dll");
const resourceDir = ref("resource");
const entry = ref("Demo");
const controller = ref("none");

// 图编辑器
const document = ref<PipelineDocument>({});
const selectedNode = ref<string | null>(null);
const saveVersion = ref("V2");

const logs = ref<string[]>([]);
let unsubscribe: (() => void) | null = null;

function log(message: string) {
  logs.value.unshift(`[${new Date().toLocaleTimeString()}] ${message}`);
}

async function run(label: string, action: () => Promise<string>) {
  busy.value = true;
  try {
    log(`${label}：${await action()}`);
  } catch (error) {
    log(`${label}失败：${String(error)}`);
  } finally {
    busy.value = false;
  }
}

/* ---------------- 图编辑器 ---------------- */

async function refreshDocument() {
  document.value = await pipelineGet();
}

async function onOpenPipeline() {
  await run("打开资源包", () => pipelineOpen(resourceDir.value));
  await refreshDocument();
}

async function onSavePipeline() {
  await run("保存 Pipeline", () => pipelineSave(null, saveVersion.value));
}

async function onAddNode() {
  const name = await pipelineAddNode();
  log(`已新建节点 ${name}`);
  await refreshDocument();
  selectedNode.value = name;
}

async function onDeleteNode() {
  if (!selectedNode.value) {
    log("请先选中一个节点");
    return;
  }
  const name = selectedNode.value;
  await run("删除节点", () => pipelineDeleteNode(name));
  selectedNode.value = null;
  await refreshDocument();
}

async function onSaveNode(payload: { name: string; node: PipelineNodeData }) {
  await run("保存节点", () => pipelineUpdateNode(payload.name, payload.node));
  await refreshDocument();
}

/** 把抓取到的模板应用到当前选中节点，自动切换为 TemplateMatch 识别 */
async function onApplyTemplate(file: string) {
  if (!selectedNode.value) {
    log("请先选中一个节点，再应用模板");
    return;
  }
  const current = document.value[selectedNode.value];
  if (!current) {
    return;
  }
  const updated: PipelineNodeData = {
    ...current,
    recognition: { type: "TemplateMatch", param: { template: file } },
  };
  await run("应用模板", () => pipelineUpdateNode(selectedNode.value!, updated));
  await refreshDocument();
}

const selectedNodeData = (): PipelineNodeData | null => {
  if (!selectedNode.value) {
    return null;
  }
  return document.value[selectedNode.value] ?? null;
};

onMounted(async () => {
  // 订阅后端推送的节点执行事件，用于调试回显
  unsubscribe = await onMaaEvent((payload) => {
    log(`事件 ${payload.message} ${payload.detail}`);
  });
});

onUnmounted(() => {
  unsubscribe?.();
});
</script>

<template>
  <main class="page">
    <header>
      <h1>MaaWizard</h1>
      <p class="subtitle">基于 MaaFramework 的可视化录制与自动化运行工具</p>
    </header>

    <nav class="tabs">
      <button
        v-for="item in tabs"
        :key="item.key"
        :class="{ active: tab === item.key }"
        @click="tab = item.key"
      >
        {{ item.label }}
      </button>
    </nav>

    <!-- 运行 -->
    <section v-if="tab === 'run'" class="card">
      <h2>运行链路</h2>
      <div class="row">
        <input v-model="dllPath" placeholder="MaaFramework.dll 路径" />
        <button :disabled="busy" @click="run('加载库', () => loadLibrary(dllPath))">
          加载动态库
        </button>
      </div>
      <p class="hint">首次运行需先执行 <code>make fetch-sdk</code> 下载官方运行时。</p>

      <div class="row">
        <input v-model="resourceDir" placeholder="资源包目录" />
        <button :disabled="busy" @click="run('加载资源', () => loadResource(resourceDir))">
          加载资源
        </button>
      </div>

      <div class="row">
        <input v-model="entry" placeholder="入口节点名" />
        <button :disabled="busy" @click="run('运行任务', () => runTask(entry))">运行</button>
        <button :disabled="busy" @click="run('停止', () => stopTask())">停止</button>
        <button :disabled="busy" @click="run('状态', () => runtimeStatus())">查看状态</button>
      </div>
      <p class="hint">当前控制器：{{ controller }}</p>
    </section>

    <!-- 图编辑器 -->
    <section v-else-if="tab === 'editor'" class="card">
      <h2>图编辑器</h2>
      <div class="row">
        <input v-model="resourceDir" placeholder="资源包目录" />
        <button :disabled="busy" @click="onOpenPipeline">打开</button>
        <select v-model="saveVersion">
          <option value="V1">导出 V1</option>
          <option value="V2">导出 V2</option>
        </select>
        <button :disabled="busy" @click="onSavePipeline">保存</button>
        <button :disabled="busy" @click="onAddNode">新建节点</button>
        <button :disabled="busy || !selectedNode" @click="onDeleteNode">删除节点</button>
      </div>

      <GraphEditor :document="document" @select="selectedNode = $event" />

      <RoiCapture :resource-dir="resourceDir" @log="log" @apply="onApplyTemplate" />

      <div v-if="selectedNode && selectedNodeData()" class="inspector-wrap">
        <NodeInspector
          :name="selectedNode"
          :node="selectedNodeData()!"
          :controller="controller"
          @save="onSaveNode"
        />
      </div>
      <p v-else class="hint">点击画布中的节点以编辑其识别/动作参数。</p>
    </section>

    <!-- 录制 -->
    <RecorderPanel
      v-else-if="tab === 'record'"
      :resource-dir="resourceDir"
      @log="log"
      @committed="refreshDocument"
    />

    <!-- 设备 -->
    <DevicePanel v-else-if="tab === 'device'" @log="log" @controller="controller = $event" />

    <!-- AI -->
    <AiPanel v-else-if="tab === 'ai'" @log="log" />

    <section class="card">
      <h2>日志</h2>
      <div class="logs">
        <p v-for="(item, index) in logs" :key="index">{{ item }}</p>
        <p v-if="logs.length === 0" class="muted">暂无日志</p>
      </div>
    </section>
  </main>
</template>

<style scoped>
.page {
  max-width: 1000px;
  margin: 0 auto;
  padding: 20px 18px 48px;
  font-family: "Segoe UI", "Microsoft YaHei", system-ui, sans-serif;
  color: #1f2933;
}
.subtitle {
  margin: 4px 0 0;
  color: #6b7280;
  font-size: 13px;
}
.tabs {
  display: flex;
  gap: 6px;
  margin: 16px 0;
  flex-wrap: wrap;
}
.tabs button {
  padding: 7px 16px;
  border: 1px solid #d1d5db;
  background: #fff;
  border-radius: 999px;
  cursor: pointer;
  font-size: 13px;
}
.tabs button.active {
  background: #2563eb;
  border-color: #2563eb;
  color: #fff;
}
.card {
  padding: 16px;
  border: 1px solid #e5e7eb;
  border-radius: 10px;
  background: #fff;
  margin-bottom: 14px;
}
.card h2 {
  margin: 0 0 12px;
  font-size: 16px;
}
.row {
  display: flex;
  gap: 8px;
  margin-bottom: 10px;
  flex-wrap: wrap;
}
input,
select {
  padding: 7px 10px;
  border: 1px solid #d1d5db;
  border-radius: 6px;
  font-size: 13px;
}
input {
  flex: 1;
  min-width: 180px;
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
.hint {
  margin: 4px 0 10px;
  font-size: 12px;
  color: #6b7280;
}
.inspector-wrap {
  margin-top: 14px;
}
.logs {
  max-height: 260px;
  overflow-y: auto;
  font-family: Consolas, "Courier New", monospace;
  font-size: 12px;
  line-height: 1.6;
}
.logs p {
  margin: 0;
}
.muted {
  color: #9ca3af;
}
</style>
