import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** 后端返回的 ADB 设备信息 */
export interface AdbDeviceInfo {
  adb_path: string;
  address: string;
  config: string;
}

/** 后端返回的桌面窗口信息 */
export interface WindowInfo {
  hwnd: number;
  class_name: string;
  window_name: string;
}

/** 录制得到的单个操作步骤 */
export interface RecordedStep {
  kind: string;
  x: number | null;
  y: number | null;
  end_x: number | null;
  end_y: number | null;
  duration_ms: number | null;
  text: string | null;
  key: string | null;
  template: string | null;
  roi: number[] | null;
}

/** AI 运行环境探测结果 */
export interface AiEnvironment {
  python: string | null;
  uv: string | null;
  uvx: string | null;
  usable: boolean;
  suggestion: string;
}

/** Pipeline 节点（与后端 PipelineNode 对应） */
export interface PipelineNodeData {
  recognition?: { type: string; param: Record<string, unknown> } | string;
  action?: { type: string; param: Record<string, unknown> } | string;
  next?: unknown[];
  on_error?: unknown[];
  [key: string]: unknown;
}

/** Pipeline 文档：节点名 → 节点 */
export type PipelineDocument = Record<string, PipelineNodeData>;

/** 校验问题（对应后端 pipeline/validate.rs 的 ValidationIssue） */
export interface ValidationIssue {
  /** 节点名；文档级别问题为空字符串 */
  node: string;
  level: "error" | "warning";
  /** 字段路径，如 recognition.template */
  field: string;
  message: string;
}

/**
 * 封装所有 Tauri 命令。
 * 前端不持有 Maa 对象，全部通过命令读写后端状态（见 ADR 0002）。
 */

/* M0 运行时 */
export const loadLibrary = (dllPath: string) =>
  invoke<string>("maa_load_library", { dllPath });

export const findAdbDevices = () => invoke<AdbDeviceInfo[]>("maa_find_adb_devices");

export const connectAdb = (adbPath: string, address: string, config: string) =>
  invoke<string>("maa_connect_adb", { adbPath, address, config });

export const connectWin32 = (hwnd: number) =>
  invoke<string>("maa_connect_win32", { hwnd });

export const loadResource = (path: string) =>
  invoke<string>("maa_load_resource", { path });

export const runTask = (entry: string, resourceDir: string) =>
  invoke<string>("maa_run_task", { entry, resourceDir });

export const stopTask = () => invoke<string>("maa_stop");

export const runtimeStatus = () => invoke<string>("maa_status");

/** 截一帧控制器画面并保存到文件，返回路径（用于展示「当前屏幕」） */
export const controllerScreenshot = (output: string) =>
  invoke<string>("maa_controller_screenshot", { output });

/* M1 图编辑器 */
export const pipelineOpen = (path: string) =>
  invoke<string>("pipeline_open", { path });

export const pipelineSave = (path: string | null, version: string) =>
  invoke<string>("pipeline_save", { path, version });

export const pipelineGet = () => invoke<PipelineDocument>("pipeline_get");

export const pipelineUpdateNode = (name: string, node: PipelineNodeData) =>
  invoke<string>("pipeline_update_node", { name, node });

export const pipelineAddNode = (name?: string) =>
  invoke<string>("pipeline_add_node", { name: name ?? null });

export const pipelineDeleteNode = (name: string) =>
  invoke<string>("pipeline_delete_node", { name });

/** 校验当前文档，返回可定位到「节点 + 字段」的问题列表 */
export const pipelineValidate = () => invoke<ValidationIssue[]>("pipeline_validate");

/* M2/M3 录制与模板抓取 */
export const recorderStart = (mode: string, resourceDir: string) =>
  invoke<string>("recorder_start", { mode, resourceDir });

export const recorderStop = () => invoke<RecordedStep[]>("recorder_stop");

export const recorderCommit = () => invoke<string>("recorder_commit");

export const captureGrabTemplate = (
  x: number,
  y: number,
  width: number,
  height: number,
  resourceDir: string,
) =>
  invoke<{ file: string; roi: number[] }>("capture_grab_template", {
    x,
    y,
    width,
    height,
    resourceDir,
  });

export const captureScreenshot = (output: string) =>
  invoke<string>("capture_screenshot", { output });

/* M4 设备管理 */
export const deviceListWindows = () => invoke<WindowInfo[]>("device_list_windows");

/* 资源与模板 */
export const listResources = () => invoke<string[]>("list_resources");

/** 解析模板匹配用到的模板图片真实路径（绝对路径），便于前端展示匹配的是哪张图 */
export const templateImage = (resourceDir: string, template: string) =>
  invoke<string>("template_image", { resourceDir, template });

/* M6 AI 增强 */
export const aiDetect = () => invoke<AiEnvironment>("ai_detect");

export const aiRun = (program: string, args: string[]) =>
  invoke<string>("ai_run", { program, args });

/**
 * 订阅后端推送的运行事件（阶段 4 调试回显）。
 * 当识别命中时，payload 额外携带 node（节点名）、hit（是否命中）、
 * box（[x, y, w, h] 识别框）、image（命中风存盘路径），便于前端叠加展示「匹配到了什么」。
 * 返回取消订阅函数，组件卸载时调用。
 */
export function onMaaEvent(
  handler: (payload: {
    message: string;
    detail: string;
    node: string;
    hit: boolean;
    box: number[] | null;
    image: string;
  }) => void,
) {
  return listen<{
    message: string;
    detail: string;
    node: string;
    hit: boolean;
    box: number[] | null;
    image: string;
  }>("maa://event", (event) => {
    handler(event.payload);
  });
}
