<script setup lang="ts">
import { ref } from "vue";
import {
  connectAdb,
  connectWin32,
  deviceListWindows,
  findAdbDevices,
  runtimeStatus,
  type AdbDeviceInfo,
  type WindowInfo,
} from "../api/maa";

const emit = defineEmits<{
  (event: "log", message: string): void;
  (event: "controller", type: string): void;
}>();

const devices = ref<AdbDeviceInfo[]>([]);
const windows = ref<WindowInfo[]>([]);
const selectedDevice = ref<AdbDeviceInfo | null>(null);
const selectedWindow = ref<WindowInfo | null>(null);
const status = ref("");
const busy = ref(false);

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
    return result;
  });
}

void refreshStatus();
</script>

<template>
  <section class="panel">
    <h3>连接控制设备</h3>
    <p v-if="status" class="status" :class="{ ok: status.includes('控制器已连接=true'), bad: status.includes('控制器未连接') }">
      {{ status }}
    </p>

    <h4>桌面窗口（Win32）</h4>
    <div class="row">
      <button :disabled="busy" @click="onRefreshWindows">刷新窗口</button>
      <button :disabled="busy || !selectedWindow" @click="onConnectWindow">连接选中窗口</button>
    </div>
    <ul class="list">
      <li
        v-for="window in windows"
        :key="window.hwnd"
        :class="{ selected: selectedWindow?.hwnd === window.hwnd }"
        @click="selectedWindow = window"
      >
        {{ window.window_name }}
        <span class="muted">hwnd={{ window.hwnd }}</span>
      </li>
      <li v-if="windows.length === 0" class="muted">暂无窗口，点击刷新</li>
    </ul>

    <h4>ADB 设备（Android）</h4>
    <div class="row">
      <button :disabled="busy" @click="onRefreshDevices">刷新设备</button>
      <button :disabled="busy || !selectedDevice" @click="onConnectAdb">连接选中设备</button>
    </div>
    <ul class="list">
      <li
        v-for="device in devices"
        :key="device.address"
        :class="{ selected: selectedDevice?.address === device.address }"
        @click="selectedDevice = device"
      >
        {{ device.address }}
      </li>
      <li v-if="devices.length === 0" class="muted">暂无设备，点击刷新</li>
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
