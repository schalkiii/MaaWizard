use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, State};

use crate::ai;
use crate::capture;
use crate::device;
use crate::maa::{resolve_existing_path, resolve_existing_path_allow_missing, AdbDeviceInfo, MaaRuntime};
use crate::pipeline::{NextEntry, PipelineState, PipelineVersion, ValidationIssue};
use crate::recorder::{RecordMode, RecorderState, RecordedStep};

/* ---------------- M0 运行时 ---------------- */

#[tauri::command]
pub fn maa_load_library(runtime: State<'_, MaaRuntime>, dll_path: String) -> Result<String, String> {
    runtime.load_library(&dll_path)
}

#[tauri::command]
pub fn maa_find_adb_devices(runtime: State<'_, MaaRuntime>) -> Result<Vec<AdbDeviceInfo>, String> {
    runtime.find_adb_devices()
}

#[tauri::command]
pub fn maa_connect_adb(
    runtime: State<'_, MaaRuntime>,
    adb_path: String,
    address: String,
    config: String,
) -> Result<String, String> {
    runtime.connect_adb(&adb_path, &address, &config)
}

#[tauri::command]
pub fn maa_connect_win32(runtime: State<'_, MaaRuntime>, hwnd: i64) -> Result<String, String> {
    runtime.connect_win32(hwnd)
}

#[tauri::command]
pub fn maa_load_resource(runtime: State<'_, MaaRuntime>, path: String) -> Result<String, String> {
    runtime.load_resource(&path)
}

/// 运行任务；同时注册事件回调把节点执行状态推送给前端（阶段 4 调试回显）。
/// `resource_dir` 用于在识别命中时把当前帧存盘，供前端直观展示「匹配到了什么」。
#[tauri::command]
pub async fn maa_run_task(
    app: AppHandle,
    runtime: State<'_, MaaRuntime>,
    entry: String,
    resource_dir: String,
) -> Result<String, String> {
    let tasker = runtime.tasker_clone()?;
    // 控制器克隆出来放进 sink 闭包：识别命中时抓一帧并保存
    let controller = runtime.controller_clone().ok();

    // Tasker 的 sink 会收到每个节点的识别/动作事件，转发为前端事件用于高亮
    let event_app = app.clone();
    let snapshot_dir = resolve_existing_path_allow_missing(&resource_dir);
    let _ = tasker.add_sink(move |message: &str, detail: &str| {
        let mut node = String::new();
        let mut hit = false;
        let mut recognition_box: Option<Vec<i32>> = None;
        let mut image_path = String::new();

        if message == "NodeDetail" {
            if let Ok(value) = serde_json::from_str::<Value>(detail) {
                if let Some(name) = value.get("id").and_then(|v| v.as_str()) {
                    node = name.to_string();
                } else if let Some(rec) = value.get("recognition") {
                    if let Some(name) = rec.get("name").and_then(|v| v.as_str()) {
                        node = name.to_string();
                    }
                }
                if let Some(rec) = value.get("recognition") {
                    hit = rec.get("hit").and_then(|v| v.as_bool()).unwrap_or(false);
                    if let Some(array) = rec.get("box").and_then(|v| v.as_array()) {
                        let nums: Vec<i32> =
                            array.iter().filter_map(|v| v.as_i64().map(|n| n as i32)).collect();
                        if nums.len() == 4 {
                            recognition_box = Some(nums);
                        }
                    }
                }
            }
        }

        // 识别命中时，抓当前帧存盘，前端可叠加识别框直观看到匹配结果
        if hit {
            if let Some(controller) = &controller {
                if let Ok(job) = controller.post_screencap() {
                    let _ = controller.wait(job);
                    if let Ok(image) = controller.cached_image() {
                        let path = snapshot_dir
                            .join(format!(".recognize_{}.png", chrono::Utc::now().timestamp_millis()));
                        let _ = std::fs::create_dir_all(&snapshot_dir);
                        if crate::capture::save_maa_image(&image, &path).is_ok() {
                            image_path = path.to_string_lossy().to_string();
                        }
                    }
                }
            }
        }

        let _ = event_app.emit(
            "maa://event",
            json!({
                "message": message,
                "detail": detail,
                "node": node,
                "hit": hit,
                "box": recognition_box,
                "image": image_path,
            }),
        );
    });

    tauri::async_runtime::spawn_blocking(move || MaaRuntime::run_task_blocking(tasker, &entry))
        .await
        .map_err(|e| format!("任务线程异常: {}", e))?
}

#[tauri::command]
pub fn maa_stop(runtime: State<'_, MaaRuntime>) -> Result<String, String> {
    runtime.stop()
}

#[tauri::command]
pub fn maa_status(runtime: State<'_, MaaRuntime>) -> Result<String, String> {
    runtime.status()
}

/// 截一帧控制器画面并保存到文件，前端用于展示「当前屏幕」
#[tauri::command]
pub fn maa_controller_screenshot(
    runtime: State<'_, MaaRuntime>,
    output: String,
) -> Result<String, String> {
    runtime.controller_screenshot(&output)
}

/* ---------------- M1 图编辑器 ---------------- */

#[tauri::command]
pub fn pipeline_open(pipeline: State<'_, PipelineState>, path: String) -> Result<String, String> {
    pipeline.open(&path)
}

#[tauri::command]
pub fn pipeline_save(
    pipeline: State<'_, PipelineState>,
    path: Option<String>,
    version: Option<String>,
) -> Result<String, String> {
    let version = match version {
        Some(text) => PipelineVersion::from_str_checked(&text)?,
        None => PipelineVersion::V2,
    };
    pipeline.save(path, version)
}

#[tauri::command]
pub fn pipeline_get(pipeline: State<'_, PipelineState>) -> Result<Value, String> {
    pipeline.snapshot()
}

#[tauri::command]
pub fn pipeline_update_node(
    pipeline: State<'_, PipelineState>,
    name: String,
    node: Value,
) -> Result<String, String> {
    pipeline.update_node(&name, node)
}

#[tauri::command]
pub fn pipeline_add_node(
    pipeline: State<'_, PipelineState>,
    name: Option<String>,
) -> Result<String, String> {
    pipeline.add_node(name)
}

#[tauri::command]
pub fn pipeline_delete_node(
    pipeline: State<'_, PipelineState>,
    name: String,
) -> Result<String, String> {
    pipeline.delete_node(&name)
}

/// 校验当前文档；返回可定位到「节点 + 字段」的问题列表，供前端在保存/运行前提示
#[tauri::command]
pub fn pipeline_validate(
    pipeline: State<'_, PipelineState>,
) -> Result<Vec<ValidationIssue>, String> {
    pipeline.validate()
}

/* ---------------- M2/M3 录制与模板抓取 ---------------- */

#[tauri::command]
pub fn recorder_start(
    recorder: State<'_, RecorderState>,
    mode: String,
    resource_dir: String,
) -> Result<String, String> {
    recorder.start(RecordMode::parse(&mode), &resource_dir)
}

/// 停止录制并返回录到的步骤，供前端预览后决定是否写入 pipeline
#[tauri::command]
pub fn recorder_stop(recorder: State<'_, RecorderState>) -> Result<Vec<RecordedStep>, String> {
    recorder.stop()
}

#[tauri::command]
pub fn recorder_status(recorder: State<'_, RecorderState>) -> Result<bool, String> {
    Ok(recorder.is_recording())
}

/// 把录制步骤转换为节点并串联写入 PipelineDocument —— 录完即得到 pipeline
#[tauri::command]
pub fn recorder_commit(
    pipeline: State<'_, PipelineState>,
    recorder: State<'_, RecorderState>,
) -> Result<String, String> {
    let steps = recorder.stop()?;
    let nodes = crate::recorder::steps_to_nodes(&steps);
    if nodes.is_empty() {
        return Err("没有录制到任何操作".to_string());
    }

    let mut document = pipeline.document_guard()?;
    let mut previous: Option<String> = None;
    let mut added = 0usize;

    for node in nodes {
        let name = document.unique_name("Step");
        // 用 next 把节点按录制顺序串成链
        if let Some(previous_name) = &previous {
            if let Some(previous_node) = document.nodes.get_mut(previous_name) {
                previous_node.next = vec![NextEntry::Name(name.clone())];
            }
        }
        document.nodes.insert(name.clone(), node);
        previous = Some(name);
        added += 1;
    }

    Ok(format!("已生成 {} 个节点并按录制顺序串联", added))
}

/// 按矩形抓取模板图，用于编辑器中的 ROI 框选（M3）
#[tauri::command]
pub fn capture_grab_template(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    resource_dir: String,
) -> Result<Value, String> {
    let screen = capture::capture_desktop()?;
    let (image, roi) = capture::crop_rect(&screen, x, y, width, height).ok_or("裁剪区域无效")?;

    let directory = resolve_existing_path_allow_missing(&resource_dir);
    let file_name = format!("roi_{}.png", chrono::Utc::now().timestamp_millis());
    let path = directory.join("image").join(&file_name);
    capture::save_template(&image, &path)?;

    Ok(json!({ "file": file_name, "roi": roi }))
}

/// 截屏保存到指定路径，便于前端显示后做可视化框选
#[tauri::command]
pub fn capture_screenshot(output: String) -> Result<String, String> {
    let path = resolve_existing_path_allow_missing(&output);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let screen = capture::capture_desktop()?;
    capture::save_template(&screen, &path)?;
    Ok(path.to_string_lossy().to_string())
}

/* ---------------- M4 设备管理 ---------------- */

#[tauri::command]
pub fn device_list_windows(runtime: State<'_, MaaRuntime>) -> Result<Vec<device::WindowInfo>, String> {
    runtime.ensure_loaded()?;
    device::list_windows()
}

/* ---------------- 资源与模板 ---------------- */

/// 列出可选的资源包目录：默认 `resource` 本身，加上它下面的一级子目录
/// （每个子目录通常是一个 Maa 资源包，内含 pipeline/ 与 image/）。
#[tauri::command]
pub fn list_resources() -> Result<Vec<String>, String> {
    let root = resolve_existing_path("resource");
    let mut dirs = vec!["resource".to_string()];
    if root.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        dirs.push(format!("resource/{}", name));
                    }
                }
            }
        }
    }
    Ok(dirs)
}

/// 解析模板匹配用到的模板图片真实路径。模板名在 pipeline 里通常不带扩展名，
/// 这里按常见扩展名依次尝试，便于前端直接展示「模板匹配到底匹配的是哪张图」。
#[tauri::command]
pub fn template_image(resource_dir: String, template: String) -> Result<String, String> {
    let root = resolve_existing_path(&resource_dir);
    let image_dir = root.join("image");
    let bases: Vec<std::path::PathBuf> = if image_dir.is_dir() {
        vec![image_dir.clone()]
    } else {
        vec![root.clone()]
    };
    let exts = ["", ".png", ".jpg", ".jpeg", ".bmp"];
    for base in &bases {
        for ext in exts {
            let candidate = base.join(format!("{}{}", template, ext));
            if candidate.is_file() {
                return Ok(candidate.to_string_lossy().to_string());
            }
        }
    }
    Err(format!(
        "找不到模板图片：{}/image/{}（已尝试 png/jpg/jpeg/bmp）",
        root.display(),
        template
    ))
}

/* ---------------- M5 AI 增强 ---------------- */

#[tauri::command]
pub fn ai_detect() -> Result<ai::AiEnvironment, String> {
    Ok(ai::detect_environment())
}

#[tauri::command]
pub fn ai_run(program: String, args: Vec<String>) -> Result<String, String> {
    ai::run_tool(program, args)
}
