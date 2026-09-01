<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { PipelineNodeData } from "../api/maa";
import {
  ACTION_HELP,
  ACTION_TYPES,
  contextualHints,
  defaultParam,
  NODE_FIELD_HELP,
  RECOGNITION_HELP,
  RECOGNITION_TYPES,
} from "../help/registry";

const props = defineProps<{
  name: string;
  node: PipelineNodeData;
  controller: string;
}>();
const emit = defineEmits<{
  (event: "save", payload: { name: string; node: PipelineNodeData }): void;
}>();

/** 从 V1/V2 两种写法中取出类型名与参数表 */
function readSpec(spec: unknown): { type: string; param: Record<string, unknown> } {
  if (typeof spec === "string") {
    return { type: spec, param: {} };
  }
  if (spec && typeof spec === "object") {
    const record = spec as Record<string, unknown>;
    return {
      type: String(record.type ?? "DirectHit"),
      param: (record.param as Record<string, unknown>) ?? {},
    };
  }
  return { type: "DirectHit", param: {} };
}

const recognitionType = ref("DirectHit");
const recognitionParam = ref("{}");
const actionType = ref("DoNothing");
const actionParam = ref("{}");
const nextList = ref<string[]>([]);
const errorList = ref<string[]>([]);
const timeout = ref<number | null>(null);
const preDelay = ref<number | null>(null);
const postDelay = ref<number | null>(null);
const inverse = ref(false);
const jsonError = ref("");

// 选中节点变化时，把后端数据载入表单
watch(
  () => [props.name, props.node],
  () => {
    const recognition = readSpec(props.node.recognition);
    const action = readSpec(props.node.action);
    recognitionType.value = recognition.type;
    recognitionParam.value = JSON.stringify(recognition.param, null, 2);
    actionType.value = action.type;
    actionParam.value = JSON.stringify(action.param, null, 2);
    nextList.value = (props.node.next ?? []).map(String);
    errorList.value = (props.node.on_error ?? []).map(String);
    timeout.value = (props.node.timeout as number) ?? null;
    preDelay.value = (props.node.pre_delay as number) ?? null;
    postDelay.value = (props.node.post_delay as number) ?? null;
    inverse.value = Boolean(props.node.inverse);
    jsonError.value = "";
  },
  { immediate: true, deep: true },
);

const recognitionHelp = computed(() => RECOGNITION_HELP[recognitionType.value]);
const actionHelp = computed(() => ACTION_HELP[actionType.value]);
const hints = computed(() =>
  contextualHints(recognitionType.value, actionType.value, props.controller),
);

/** 参数框为空或仅 {} 时，视为未填写，切换类型时预填推荐默认值 */
function isEmptyParam(value: string): boolean {
  const trimmed = value.trim();
  return trimmed === "" || trimmed === "{}";
}

function onRecognitionChange() {
  if (isEmptyParam(recognitionParam.value)) {
    recognitionParam.value = defaultParam("recognition", recognitionType.value);
  }
}

function onActionChange() {
  if (isEmptyParam(actionParam.value)) {
    actionParam.value = defaultParam("action", actionType.value);
  }
}

function save() {
  let recognition: Record<string, unknown> = {};
  let action: Record<string, unknown> = {};
  try {
    recognition = JSON.parse(recognitionParam.value || "{}");
    action = JSON.parse(actionParam.value || "{}");
  } catch (error) {
    jsonError.value = `参数 JSON 解析失败：${String(error)}`;
    return;
  }
  jsonError.value = "";

  const node: PipelineNodeData = {
    recognition: { type: recognitionType.value, param: recognition },
    action: { type: actionType.value, param: action },
    next: nextList.value.filter((item) => item.trim() !== ""),
    on_error: errorList.value.filter((item) => item.trim() !== ""),
  };
  if (timeout.value !== null) node.timeout = timeout.value;
  if (preDelay.value !== null) node.pre_delay = preDelay.value;
  if (postDelay.value !== null) node.post_delay = postDelay.value;
  if (inverse.value) node.inverse = true;

  emit("save", { name: props.name, node });
}

function addEntry(list: "next" | "error") {
  if (list === "next") {
    nextList.value.push("");
  } else {
    errorList.value.push("");
  }
}

function removeEntry(list: "next" | "error", index: number) {
  if (list === "next") {
    nextList.value.splice(index, 1);
  } else {
    errorList.value.splice(index, 1);
  }
}
</script>

<template>
  <section class="inspector">
    <h3>
      节点：{{ name }}
      <span v-if="hints.length" class="badge">{{ hints.length }} 条提示</span>
    </h3>

    <div v-for="hint in hints" :key="hint" class="hint">{{ hint }}</div>

    <label>
      识别类型
      <select v-model="recognitionType" @change="onRecognitionChange">
        <option v-for="item in RECOGNITION_TYPES" :key="item" :value="item">{{ item }}</option>
      </select>
    </label>
    <p v-if="recognitionHelp" class="help">
      <b>效果：</b>{{ recognitionHelp.effect }} <b>用途：</b>{{ recognitionHelp.scene }}
    </p>
    <ul v-if="recognitionHelp" class="params">
      <li v-for="param in recognitionHelp.params" :key="param.name">
        <code>{{ param.name }}</code>：{{ param.desc }}
      </li>
    </ul>
    <textarea v-model="recognitionParam" rows="4" spellcheck="false" />
    <p class="example">推荐默认值：<code>{{ defaultParam("recognition", recognitionType) }}</code></p>

    <label>
      动作类型
      <select v-model="actionType" @change="onActionChange">
        <option v-for="item in ACTION_TYPES" :key="item" :value="item">{{ item }}</option>
      </select>
    </label>
    <p v-if="actionHelp" class="help">
      <b>效果：</b>{{ actionHelp.effect }} <b>用途：</b>{{ actionHelp.scene }}
    </p>
    <ul v-if="actionHelp" class="params">
      <li v-for="param in actionHelp.params" :key="param.name">
        <code>{{ param.name }}</code>：{{ param.desc }}
      </li>
    </ul>
    <textarea v-model="actionParam" rows="4" spellcheck="false" />
    <p class="example">推荐默认值：<code>{{ defaultParam("action", actionType) }}</code></p>

    <div class="grid">
      <label>timeout<input v-model.number="timeout" type="number" /></label>
      <label>pre_delay<input v-model.number="preDelay" type="number" /></label>
      <label>post_delay<input v-model.number="postDelay" type="number" /></label>
      <label class="checkbox">inverse<input v-model="inverse" type="checkbox" /></label>
    </div>

    <fieldset>
      <legend>
        next（识别成功后的后继）
        <button type="button" @click="addEntry('next')">+</button>
      </legend>
      <div v-for="(_, index) in nextList" :key="index" class="entry">
        <input v-model="nextList[index]" placeholder="节点名 或 [JumpBack]" />
        <button type="button" @click="removeEntry('next', index)">×</button>
      </div>
    </fieldset>

    <fieldset>
      <legend>
        on_error（超时/失败时跳转）
        <button type="button" @click="addEntry('error')">+</button>
      </legend>
      <div v-for="(_, index) in errorList" :key="index" class="entry">
        <input v-model="errorList[index]" placeholder="节点名" />
        <button type="button" @click="removeEntry('error', index)">×</button>
      </div>
    </fieldset>

    <details>
      <summary>节点公共字段说明</summary>
      <ul class="params">
        <li v-for="field in NODE_FIELD_HELP" :key="field.name">
          <code>{{ field.name }}</code>：{{ field.desc }}
        </li>
      </ul>
    </details>

    <p v-if="jsonError" class="error">{{ jsonError }}</p>
    <button class="primary" @click="save">保存节点</button>
  </section>
</template>

<style scoped>
.inspector {
  padding: 14px;
  border: 1px solid #e5e7eb;
  border-radius: 8px;
  background: #fff;
  font-size: 13px;
}
h3 {
  margin: 0 0 10px;
  font-size: 15px;
  display: flex;
  align-items: center;
  gap: 8px;
}
.badge {
  font-size: 11px;
  background: #fef3c7;
  color: #92400e;
  padding: 2px 6px;
  border-radius: 10px;
}
label {
  display: block;
  margin: 10px 0 4px;
  font-weight: 600;
}
select,
input,
textarea {
  width: 100%;
  padding: 6px 8px;
  border: 1px solid #d1d5db;
  border-radius: 6px;
  font-size: 13px;
  box-sizing: border-box;
}
textarea {
  font-family: Consolas, monospace;
  font-size: 12px;
}
.help {
  margin: 4px 0;
  color: #374151;
  background: #f3f4f6;
  padding: 6px 8px;
  border-radius: 6px;
}
.params {
  margin: 4px 0 8px;
  padding-left: 18px;
  color: #4b5563;
  font-size: 12px;
}
.hint {
  background: #fffbeb;
  border-left: 3px solid #f59e0b;
  padding: 6px 8px;
  margin-bottom: 6px;
  color: #92400e;
  font-size: 12px;
}
.grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 8px;
}
.checkbox {
  display: flex;
  align-items: center;
  gap: 6px;
}
.checkbox input {
  width: auto;
}
fieldset {
  border: 1px solid #e5e7eb;
  border-radius: 6px;
  padding: 8px;
  margin-top: 10px;
}
legend {
  font-size: 12px;
  font-weight: 600;
}
.entry {
  display: flex;
  gap: 6px;
  margin-bottom: 6px;
}
.entry button {
  padding: 0 8px;
  border: 1px solid #d1d5db;
  background: #fff;
  border-radius: 6px;
  cursor: pointer;
}
.primary {
  margin-top: 12px;
  width: 100%;
  padding: 8px;
  border: none;
  border-radius: 6px;
  background: #2563eb;
  color: #fff;
  cursor: pointer;
}
.error {
  color: #dc2626;
  font-size: 12px;
}
.example {
  margin: 4px 0 0;
  font-size: 11px;
  color: #9ca3af;
  word-break: break-all;
}
.example code {
  background: #f3f4f6;
  padding: 1px 4px;
  border-radius: 4px;
}
</style>
