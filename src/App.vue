<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { convertFileSrc } from "@tauri-apps/api/core";
import DevicePanel from "./components/DevicePanel.vue";
import GraphEditor from "./components/GraphEditor.vue";
import type { EdgeKind, NodePosition } from "./components/graph";
import NodeInspector from "./components/NodeInspector.vue";
import RecorderPanel from "./components/RecorderPanel.vue";
import RoiCapture from "./components/RoiCapture.vue";
import {
  controllerScreenshot,
  createResourceBundle,
  listResources,
  loadLibrary,
  loadResource,
  onMaaEvent,
  pipelineAddNode,
  pipelineDeleteNode,
  pipelineGet,
  pipelineOpen,
  pipelineSave,
  pipelineUpdateNode,
  pipelineValidate,
  runTask,
  runtimeStatus,
  stopTask,
  templateImage,
  type PipelineDocument,
  type PipelineNodeData,
  type ValidationIssue,
} from "./api/maa";

type TabKey = "run" | "editor" | "record" | "device";

const tabs: Array<{ key: TabKey; label: string }> = [
  { key: "run", label: "运行" },
  { key: "editor", label: "图编辑器" },
  { key: "record", label: "录制" },
  { key: "device", label: "设备" },
];

const tab = ref<TabKey>("run");
const busy = ref(false);

// 运行时
const dllPath = ref("maa-sdk/bin/MaaFramework.dll");
const resourceDir = ref("resource");
const resourceOptions = ref<string[]>(["resource"]);
const entry = ref("Demo");
const controller = ref("none");

// 运行时的识别回显：识别命中后会拿到命中风截图 + 识别框，用于直观展示「匹配到了什么」
const recognizeImage = ref("");
const recognizeNode = ref("");
const recognizeHit = ref(false);
const recognizeBox = ref<number[] | null>(null);
const screenImage = ref("");

// 图编辑器
const document = ref<PipelineDocument>({});
const selectedNode = ref<string | null>(null);
const saveVersion = ref("V2");
/** 节点名 → 模板图片地址（按资源目录解析后），供图编辑器预览 TemplateMatch 匹配的图 */
const templateImages = ref<Record<string, string>>({});
const newBundleName = ref("");

// 校验结果
const issues = ref<ValidationIssue[]>([]);
const validated = ref(false);
const errorCount = computed(
  () => issues.value.filter((issue) => issue.level === "error").length,
);

/** 用户在画布上手动摆放的节点位置，持久化到 localStorage，避免刷新后重排 */
const POSITION_KEY = "maawizard.positions";

function loadPositions(): Record<string, NodePosition> {
  try {
    return JSON.parse(localStorage.getItem(POSITION_KEY) ?? "{}") as Record<string, NodePosition>;
  } catch {
    return {};
  }
}

const positions = ref<Record<string, NodePosition>>(loadPositions());

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
  // 文档变化后刷新：入口节点下拉选项 + 模板图预览
  syncEntryDefault();
  await refreshTemplateImages();
}

/** 入口节点下拉：用当前文档里的全部节点名作为可选项 */
const entryOptions = computed(() => Object.keys(document.value));

/** 文档加载后，若当前入口名不在节点列表里，自动选中第一个节点 */
function syncEntryDefault() {
  const keys = Object.keys(document.value);
  if (keys.length > 0 && !keys.includes(entry.value)) {
    entry.value = keys[0];
  }
}

/** 为所有 TemplateMatch 节点解析模板图片地址，供图编辑器预览 */
async function refreshTemplateImages() {
  const map: Record<string, string> = {};
  for (const [name, node] of Object.entries(document.value)) {
    const reco =
      typeof node.recognition === "string"
        ? { type: node.recognition, param: {} }
        : (node.recognition ?? {});
    const param = (reco as Record<string, unknown>).param as Record<string, unknown> | undefined;
    if ((reco as Record<string, unknown>).type === "TemplateMatch" && param?.template) {
      const tpl = String(param.template);
      try {
        const abs = await templateImage(resourceDir.value, tpl);
        map[name] = convertFileSrc(abs);
      } catch {
        // 模板图缺失时留空，不阻断编辑
      }
    }
  }
  templateImages.value = map;
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

/** 校验当前文档，把问题定位到节点与字段 */
async function onValidate() {
  busy.value = true;
  try {
    issues.value = await pipelineValidate();
    validated.value = true;
    const warnings = issues.value.length - errorCount.value;
    log(`校验完成：${errorCount.value} 个错误、${warnings} 个提示`);
  } catch (error) {
    log(`校验失败：${String(error)}`);
  } finally {
    busy.value = false;
  }
}

/** 取出连线列表中的节点名（兼容字符串与 {name} 对象两种写法） */
function entryName(item: unknown): unknown {
  if (typeof item === "string") {
    return item;
  }
  if (item && typeof item === "object") {
    return (item as Record<string, unknown>).name;
  }
  return null;
}

/** 画布上拖拽连线：把目标节点写进源节点的 next 或 on_error */
async function onConnect(payload: { source: string; target: string; kind: EdgeKind }) {
  const node = document.value[payload.source];
  if (!node) {
    return;
  }
  const list = ((node[payload.kind] as unknown[]) ?? []).map(String);
  if (list.includes(payload.target)) {
    log(`连线已存在：${payload.source} -> ${payload.target}`);
    return;
  }
  const updated: PipelineNodeData = { ...node, [payload.kind]: [...list, payload.target] };
  await run("新增连线", () => pipelineUpdateNode(payload.source, updated));
  await refreshDocument();
}

/** 删除画布连线：从源节点的 next / on_error 中移除目标 */
async function onDisconnect(payload: { source: string; target: string; kind: EdgeKind }) {
  const node = document.value[payload.source];
  if (!node) {
    return;
  }
  const list = ((node[payload.kind] as unknown[]) ?? []).filter(
    (item) => entryName(item) !== payload.target,
  );
  const updated: PipelineNodeData = { ...node, [payload.kind]: list };
  await run("删除连线", () => pipelineUpdateNode(payload.source, updated));
  await refreshDocument();
}

function onNodeMove(payload: { name: string; position: NodePosition }) {
  positions.value = { ...positions.value, [payload.name]: payload.position };
  try {
    localStorage.setItem(POSITION_KEY, JSON.stringify(positions.value));
  } catch {
    // 存储不可用时忽略：位置只影响画布排布，不影响 pipeline 内容
  }
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
  // 加载可选资源包目录，供「加载资源」下拉框使用
  try {
    resourceOptions.value = await listResources();
  } catch {
    resourceOptions.value = ["resource"];
  }
  // 订阅后端推送的节点执行事件，回显识别结果与命中截图
  unsubscribe = await onMaaEvent((payload) => {
    if (payload.node) {
      log(`节点 ${payload.node} ${payload.hit ? "命中" : "未命中"}`);
    } else {
      log(`事件 ${payload.message}`);
    }
    if (payload.image) {
      recognizeImage.value = `${convertFileSrc(payload.image)}?t=${Date.now()}`;
      recognizeNode.value = payload.node;
      recognizeHit.value = payload.hit;
      recognizeBox.value = payload.box;
    }
  });
});

/** 运行前若未连接控制器，先提示，避免空跑 */
async function onRunTask() {
  if (controller.value === "none") {
    log("请先在上方连接控制设备（桌面窗口或 ADB），否则无法运行");
    return;
  }
  await run("运行任务", () => runTask(entry.value, resourceDir.value));
}

/** 截取控制器当前画面，方便确认目标窗口与坐标 */
async function onCaptureScreen() {
  await run("截取屏幕", async () => {
    const path = await controllerScreenshot(`${resourceDir.value}/.screen.png`);
    screenImage.value = `${convertFileSrc(path)}?t=${Date.now()}`;
    return path;
  });
}

async function onCreateBundle() {
  const name = newBundleName.value.trim();
  if (!name) {
    log("请输入资源包名称");
    return;
  }
  await run("新建资源包", async () => {
    const path = await createResourceBundle(name);
    resourceOptions.value = await listResources();
    resourceDir.value = path;
    newBundleName.value = "";
    await onOpenPipeline();
    return path;
  });
}

/** 识别截图显示后，按实际显示尺寸缩放识别框坐标 */
const recognizeScale = ref(1);
function onRecognizeLoad(event: Event) {
  const img = event.target as HTMLImageElement;
  if (img.naturalWidth > 0 && img.clientWidth > 0) {
    recognizeScale.value = img.clientWidth / img.naturalWidth;
  }
}
function boxStyle(box: number[] | null): Record<string, string> {
  if (!box || box.length < 4) {
    return {};
  }
  const [x, y, w, h] = box;
  const scale = recognizeScale.value;
  return {
    left: `${x * scale}px`,
    top: `${y * scale}px`,
    width: `${w * scale}px`,
    height: `${h * scale}px`,
  };
}

onUnmounted(() => {
  unsubscribe?.();
});

// 切换资源目录后，重新解析各节点的模板图预览
watch(resourceDir, () => {
  refreshTemplateImages();
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
        <select v-model="resourceDir" title="资源包目录">
          <option v-for="opt in resourceOptions" :key="opt" :value="opt">{{ opt }}</option>
        </select>
        <button :disabled="busy" @click="run('加载资源', () => loadResource(resourceDir))">
          加载资源
        </button>
        <button :disabled="busy" @click="run('状态', () => runtimeStatus())">查看状态</button>
      </div>

      <!-- 运行页只保留控制器状态摘要；具体连接操作统一到设备页，避免两页重复 -->
      <section class="controller-summary">
        <h3>控制器状态</h3>
        <p v-if="controller === 'none'" class="bad">
          未连接控制器，任务无法运行
        </p>
        <p v-else class="ok">已连接：{{ controller }}</p>
        <button @click="tab = 'device'">
          {{ controller === 'none' ? '去设备页连接' : '查看设备页' }}
        </button>
      </section>

      <div class="row">
        <select v-model="entry" title="入口节点名">
          <option v-for="opt in entryOptions" :key="opt" :value="opt">{{ opt }}</option>
        </select>
        <button :disabled="busy" @click="onRunTask">运行</button>
        <button :disabled="busy" @click="run('停止', () => stopTask())">停止</button>
        <button :disabled="busy" @click="onCaptureScreen">查看当前屏幕</button>
      </div>

      <!-- 当前屏幕：确认目标窗口与坐标是否正确 -->
      <div v-if="screenImage" class="shot">
        <img :src="screenImage" alt="当前屏幕" />
      </div>

      <!-- 识别回显：运行后这里会显示命中风截图与识别框，直观看到「匹配到了什么」 -->
      <div v-if="recognizeImage" class="shot">
        <p class="hint">
          识别预览：节点 <b>{{ recognizeNode }}</b>
          {{ recognizeHit ? "命中" : "未命中" }}
          <span v-if="recognizeBox">框=[{{ recognizeBox.join(", ") }}]</span>
        </p>
        <div class="stage">
          <img :src="recognizeImage" alt="识别结果" @load="onRecognizeLoad" />
          <div
            v-if="recognizeBox"
            class="box"
            :style="boxStyle(recognizeBox)"
          />
        </div>
      </div>
      <p v-else class="hint">
        提示：用「图编辑器 → ROI 框选」截屏并框选模板，应用到节点后会变成 TemplateMatch；
        回到此处运行，即可在这里看到匹配结果。
      </p>
    </section>

    <!-- 图编辑器 -->
    <section v-else-if="tab === 'editor'" class="card">
      <h2>图编辑器</h2>
      <div class="row">
        <select v-model="resourceDir" title="资源包目录">
          <option v-for="opt in resourceOptions" :key="opt" :value="opt">{{ opt }}</option>
        </select>
        <button :disabled="busy" @click="onOpenPipeline">打开</button>
        <input
          v-model="newBundleName"
          placeholder="新资源包名称"
          style="width: 120px"
          @keydown.enter.prevent="onCreateBundle"
        />
        <button :disabled="busy || !newBundleName.trim()" @click="onCreateBundle">新建</button>
        <select v-model="saveVersion">
          <option value="V1">导出 V1</option>
          <option value="V2">导出 V2</option>
        </select>
        <button :disabled="busy" @click="onSavePipeline">保存</button>
        <button :disabled="busy" @click="onAddNode">新建节点</button>
        <button :disabled="busy || !selectedNode" @click="onDeleteNode">删除节点</button>
        <button :disabled="busy" @click="onValidate">校验</button>
      </div>

      <!-- 校验结果：点击问题可跳到对应节点 -->
      <div v-if="issues.length > 0" class="issues">
        <h3>校验结果：{{ errorCount }} 个错误、{{ issues.length - errorCount }} 个提示</h3>
        <p
          v-for="(issue, index) in issues"
          :key="index"
          :class="['issue', issue.level]"
          @click="issue.node && (selectedNode = issue.node)"
        >
          <b>{{ issue.node || "文档" }}</b>
          <code v-if="issue.field">{{ issue.field }}</code>
          {{ issue.message }}
        </p>
      </div>
      <p v-else-if="validated" class="hint">校验通过，没有发现问题。</p>

      <GraphEditor
        :document="document"
        :issues="issues"
        :positions="positions"
        :template-images="templateImages"
        @select="selectedNode = $event"
        @connect="onConnect"
        @disconnect="onDisconnect"
        @move="onNodeMove"
      />

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
  /* 必须显式指定文字色：下面全局 button 规则的 color:#fff 会命中这里，导致白底白字 */
  color: #374151;
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
.issues {
  margin-bottom: 12px;
  padding: 10px 12px;
  border: 1px solid #e5e7eb;
  border-radius: 8px;
  background: #fff;
}
.issues h3 {
  margin: 0 0 8px;
  font-size: 14px;
}
.issue {
  margin: 0 0 4px;
  padding: 5px 8px;
  border-left: 3px solid #d1d5db;
  border-radius: 4px;
  font-size: 12px;
  cursor: pointer;
}
.issue code {
  margin: 0 6px;
  padding: 1px 4px;
  border-radius: 4px;
  background: #f3f4f6;
}
.issue.error {
  border-left-color: #dc2626;
  background: #fef2f2;
  color: #991b1b;
}
.issue.warning {
  border-left-color: #f59e0b;
  background: #fffbeb;
  color: #92400e;
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
.shot {
  margin: 12px 0;
}
.shot img {
  max-width: 100%;
  border-radius: 6px;
  border: 1px solid #e5e7eb;
}
.stage {
  position: relative;
  display: inline-block;
  max-width: 100%;
}
.stage .box {
  position: absolute;
  border: 2px solid #16a34a;
  background: rgba(22, 163, 74, 0.18);
  pointer-events: none;
}
.controller-summary {
  padding: 12px;
  border: 1px solid #e5e7eb;
  border-radius: 8px;
  background: #fafafa;
  margin-bottom: 14px;
}
.controller-summary h3 {
  margin: 0 0 8px;
  font-size: 14px;
}
.controller-summary p {
  margin: 0 0 10px;
  font-size: 13px;
}
.controller-summary .ok {
  color: #065f46;
}
.controller-summary .bad {
  color: #991b1b;
}
.controller-summary button {
  padding: 6px 12px;
  border: 1px solid #2563eb;
  border-radius: 6px;
  background: #2563eb;
  color: #fff;
  cursor: pointer;
  font-size: 13px;
}
</style>
