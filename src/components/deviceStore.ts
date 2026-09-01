/**
 * 设备连接状态的跨组件单例。
 * 运行页与图编辑器/录制页通过 tab 切换会卸载/重建组件，若状态放在组件内部，
 * 切回运行页时窗口/设备列表会被清空、需要重新刷新。这里提到模块级，
 * 使其在整个应用生命周期内持续存在。
 */
import { ref } from "vue";
import type { AdbDeviceInfo, WindowInfo } from "../api/maa";

export const windows = ref<WindowInfo[]>([]);
export const devices = ref<AdbDeviceInfo[]>([]);
export const selectedWindow = ref<WindowInfo | null>(null);
export const selectedDevice = ref<AdbDeviceInfo | null>(null);

/** 连接成功后默认只展示已连的那一项，缩短滚动条；点「切换」可临时展开全部 */
export const showAllWindows = ref(false);
export const showAllDevices = ref(false);
