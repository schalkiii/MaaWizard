<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import {
  connectAdb,
  connectWin32,
  deviceListWindows,
  findAdbDevices,
  runtimeStatus,
  type AdbDeviceInfo,
  type WindowInfo,
} from "../api/maa";
import {
  devices,
  selectedDevice,
  selectedWindow,
  showAllDevices,
  showAllWindows,
  windows,
} from "./deviceStore";

const emit = defineEmits<{
  (event: "log", message: string): void;
  (event: "controller", type: string): void;
}>();

const status = ref("");
const busy = ref(false);
let statusTimer: number | undefined;

/** 连接后只展示已连的那一项，缩短滚动条 */
const displayWindows = computed(() =>
  showAllWindows.value || !selectedWindow.value ? windows.value : [selectedWindow.value!],
);
const displayDevices = computed(() =>
  showAllDevices.value || !selectedDevice.value ? devices.value : [selectedDevice.value!],
);

async function run(label: string, action: () => Promise<string>) {
  busy.value = true;
  try {
    const result = await action();
    emit("log", `${label}：${result}`);
    await refreshStatus();
  } catch (error) {
    emit("log", `${label}失败：${String(error)}`);
  } finally {
    busy.value = false;
  }
}

async function refreshStatus() {
  try {
    status.value = await runtimeStatus();
  } catch (error) {
    status.value = `状态获取失败：${String(error)}`;
  }
}

async function onRefreshDevices() {
  busy.value = true;
  try {
    devices.value = await findAdbDevices();
    emit("log", `发现 ${devices.value.length} 个 ADB 设备`);
  } catch (error) {
    emit("log", `刷新设备失败：${String(error)}`);
  } finally {
    busy.value = false;
  }
}

async function onRefreshWindows() {
  busy.value = true;
  try {
    windows.value = await deviceListWindows();
    emit("log", `发现 ${windows.value.length} 个桌面窗口`);
  } catch (error) {
    emit("log", `刷新窗口失败：${String(error)}`);
  } finally {
    busy.value = false;
  }
}

function onConnectAdb() {
  const device = selectedDevice.value;
  if (!device) {
    emit("log", "请先选择一个 ADB 设备");
    return;
  }
  run("连接 ADB", async () => {
    const result = await connectAdb(device.adb_path, device.address, device.config);
    emit("controller", "adb");
    showAllDevices.value = false;
    return result;
  });
}

function onConnectWindow() {
  const window = selectedWindow.value;
  if (!window) {
    emit("log", "请先选择一个窗口");
    return;
  }
  run("连接 Win32", async () => {
    const result = await connectWin32(window.hwnd);
    emit("controller", "win32");
    showAllWindows.value = false;
    return result;
  });
}

onMounted(() => {
  void refreshStatus();
  // 默认实时刷新连接状态，无需手动点击「查看状态」
  statusTimer = window.setInterval(() => void refreshStatus(), 1500);
});
onUnmounted(() => {
  if (statusTimer) {
    window.clearInterval(statusTimer);
  }
});
</script>

<template>
  <section class="panel">
    <h3>连接控制设备</h3>
    <p
      v-if="status"
      class="status"
      :class="{ ok: status.includes('控制器已连接=true'), bad: status.includes('控制器未连接') }"
    >
      {{ status }}
    </p>

    <h4>桌面窗口（Win32）</h4>
    <div class="row">
      <button :disabled="busy" @click="onRefreshWindows">刷新窗口</button>
      <button :disabled="busy || !selectedWindow" @click="onConnectWindow">连接选中窗口</button>
      <button v-if="selectedWindow && !showAllWindows" class="ghost" @click="showAllWindows = true">
        切换窗口
      </button>
    </div>
    <ul class="list">
      <li
        v-for="window in displayWindows"
        :key="window.hwnd"
        :class="{ selected: selectedWindow?.hwnd === window.hwnd }"
        @click="selectedWindow = window"
      >
        {{ window.window_name }}
        <span class="muted">hwnd={{ window.hwnd }}</span>
      </li>
      <li v-if="displayWindows.length === 0" class="muted">暂无窗口，点击刷新</li>
    </ul>

    <h4>ADB 设备（Android）</h4>
    <div class="row">
      <button :disabled="busy" @click="onRefreshDevices">刷新设备</button>
      <button :disabled="busy || !selectedDevice" @click="onConnectAdb">连接选中设备</button>
      <button v-if="selectedDevice && !showAllDevices" class="ghost" @click="showAllDevices = true">
        切换设备
      </button>
    </div>
    <ul class="list">
      <li
        v-for="device in displayDevices"
        :key="device.address"
        :class="{ selected: selectedDevice?.address === device.address }"
        @click="selectedDevice = device"
      >
        {{ device.address }}
      </li>
      <li v-if="displayDevices.length === 0" class="muted">暂无设备，点击刷新</li>
    </ul>
  </section>
</template>

<style scoped>
.panel {
  padding: 12px;
  border: 1px solid #e5e7eb;
  border-radius: 8px;
  background: #fafafa;
}
h3 {
  margin: 0 0 8px;
  font-size: 14px;
}
h4 {
  margin: 14px 0 6px;
  font-size: 13px;
  color: #374151;
}
.status {
  margin: 0 0 10px;
  padding: 8px 10px;
  border-radius: 6px;
  font-size: 12px;
  font-family: Consolas, "Courier New", monospace;
  background: #f3f4f6;
  word-break: break-all;
}
.status.ok {
  background: #ecfdf5;
  color: #065f46;
}
.status.bad {
  background: #fef2f2;
  color: #991b1b;
}
.row {
  display: flex;
  gap: 8px;
  margin-bottom: 8px;
  flex-wrap: wrap;
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
button.ghost {
  background: #fff;
  color: #2563eb;
}
.list {
  list-style: none;
  margin: 0 0 8px;
  padding: 0;
  border: 1px solid #e5e7eb;
  border-radius: 6px;
  max-height: 160px;
  overflow-y: auto;
  background: #fff;
}
.list li {
  padding: 6px 10px;
  cursor: pointer;
  font-size: 13px;
  border-bottom: 1px solid #f3f4f6;
}
.list li.selected {
  background: #eff6ff;
}
.muted {
  color: #9ca3af;
}
</style>
