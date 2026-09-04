//! MaaPipelineEditor (MPE) LocalBridge 兼容文件桥。
//!
//! MPE 前端是纯静态网页（React + React Flow），默认通过 WebSocket 连接
//! `ws://localhost:9066` 与本地"桥"通信，用来读写本地 pipeline 文件。
//! 官方 LocalBridge（Go）只实现了「文件管理」类协议；在此之上我们补齐了 MFW
//! 控制器协议的 Win32 部分（窗口枚举 / 创建控制器 / 截图 / 断开），让 MPE 内置的
//! 连接面板与实时画面（含 ROI 框选）直接可用，其余控制器类型仍由运行页负责。
//!
//! 这里用 Rust 在 Tauri 进程内起一个等价的 WS 服务，根目录指向 `resource/`，
//! 让内嵌的 MPE 前端能直接编辑我们资源包里的 pipeline JSON。

use std::collections::HashMap;
use std::os::raw::c_void;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::UNIX_EPOCH;

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use futures_util::{SinkExt, StreamExt};
use maa_framework::controller::Controller;
use maa_framework::resource::Resource;
use maa_framework::tasker::Tasker;
use maa_framework::MaaStatus;
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use crate::maa::{ensure_library_loaded, WIN32_INPUT_AUTO, WIN32_SCREENCAP_WINDOW};

/// LocalBridge 默认端口，与 MPE 前端硬编码的 `ws://localhost:9066` 一致。
pub const BRIDGE_PORT: u16 = 9066;

/// MPE 侧持有的 Win32 控制器：与运行页的控制器相互独立，互不抢占。
struct MpeController {
    hwnd: i64,
    controller: Controller,
}

/// MPE 侧控制器池。
struct MpeControllerPool {
    next_id: u64,
    map: HashMap<String, MpeController>,
}

fn controller_pool() -> &'static Mutex<MpeControllerPool> {
    static POOL: OnceLock<Mutex<MpeControllerPool>> = OnceLock::new();
    POOL.get_or_init(|| {
        Mutex::new(MpeControllerPool {
            next_id: 0,
            map: HashMap::new(),
        })
    })
}

static DEBUG_SESSION_SEQ: AtomicU64 = AtomicU64::new(0);
static DEBUG_RUN_SEQ: AtomicU64 = AtomicU64::new(0);

/// 一次进行中的调试运行：持有 Tasker 以支持「停止」。
struct DebugRunState {
    run_id: String,
    tasker: Tasker,
}

fn debug_run_state() -> &'static Mutex<Option<DebugRunState>> {
    static STATE: OnceLock<Mutex<Option<DebugRunState>>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(None))
}

fn now_iso() -> String {
    chrono::Local::now().to_rfc3339()
}

/// MPE 调试能力清单（与前端 `DebugCapabilityManifest` 对齐，camelCase 字段）。
fn debug_capabilities() -> Value {
    json!({
        "generation": "debug-vNext",
        "runModes": ["run-from-node", "single-node-run"],
        "diagnostics": [],
        "artifacts": [],
        "screenshotSources": [],
        "profileFeatures": [],
        "maa": {
            "mfwVersion": maa_framework::maa_version(),
            "supportedControllers": ["win32"],
            "supportedTaskerApis": ["post_task", "override_pipeline", "post_stop"],
            "supportedResourceApis": ["post_bundle", "override_pipeline"],
            "supportedAgentTransports": [],
        },
    })
}

fn debug_session_snapshot(session_id: &str, status: &str) -> Value {
    json!({
        "sessionId": session_id,
        "status": status,
        "createdAt": now_iso(),
        "updatedAt": now_iso(),
        "capabilities": debug_capabilities(),
    })
}

/// 组装一条待发送的 WS 文本消息（供事件通道异步推送使用）。
fn message_for(path: &str, data: Value) -> Message {
    let text = serde_json::to_string(&json!({ "path": path, "data": data })).unwrap_or_default();
    Message::text(text)
}

#[derive(Clone)]
struct Bridge {
    root: PathBuf,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct WsMessage {
    path: String,
    #[serde(default)]
    data: Value,
}

/// 在 Tauri 异步运行时中拉起文件桥服务（失败仅告警，不阻断启动）。
pub fn start() {
    tauri::async_runtime::spawn(async {
        if let Err(e) = run_server().await {
            eprintln!("[mpe-bridge] 服务异常: {e}");
        }
    });
}

async fn run_server() -> Result<(), String> {
    // 用「真实资源根」解析：跳过 dev 期 cwd 下运行时误建的空壳 resource/，
    // 保证 MPE 看到的始终是仓库根的正式资源目录。
    let root = crate::maa::resolve_resource_root();
    // 统一成绝对路径：MPE 前端可能以绝对路径访问文件，绝对根才能保证 safe_path 的
    // starts_with 判定在「相对输入」与「绝对输入」两种情形下都一致。
    let root = if root.is_absolute() {
        root
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(root)
    };
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    run_server_with_port(BRIDGE_PORT, root).await
}

async fn run_server_with_port(port: u16, root: PathBuf) -> Result<(), String> {
    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr).await.map_err(|e| e.to_string())?;
    run_server_with_listener(listener, root).await
}

async fn run_server_with_listener(listener: TcpListener, root: PathBuf) -> Result<(), String> {
    let bridge = Arc::new(Bridge { root });
    let addr = listener.local_addr().map_err(|e| e.to_string())?;
    eprintln!(
        "[mpe-bridge] 监听 {addr}（资源根目录：{}）",
        bridge.root.display()
    );

    loop {
        let (stream, _) = listener.accept().await.map_err(|e| e.to_string())?;
        let bridge = bridge.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_conn(stream, bridge).await {
                eprintln!("[mpe-bridge] 连接处理错误: {e}");
            }
        });
    }
}

async fn handle_conn(
    stream: tokio::net::TcpStream,
    bridge: Arc<Bridge>,
) -> Result<(), String> {
    let ws_stream = tokio_tungstenite::accept_async(stream)
        .await
        .map_err(|e| format!("WS 握手失败: {e}"))?;
    let (mut writer, mut reader) = ws_stream.split();

    // 事件通道：调试运行等异步流程在完成后经此把消息推回本连接的客户端
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<Message>();

    // 连接建立后先推送当前文件列表与资源包列表（LocalBridge 行为）。
    push_file_state(&mut writer, &bridge.root).await?;

    loop {
        tokio::select! {
            out = event_rx.recv() => {
                match out {
                    Some(message) => {
                        if writer.send(message).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
            msg = reader.next() => {
                let msg = match msg {
                    Some(Ok(m)) => m,
                    Some(Err(_)) | None => break,
                };
                let text = match msg {
                    Message::Text(t) => t,
                    Message::Close(_) => break,
                    Message::Ping(p) => {
                        let _ = writer.send(Message::Pong(p)).await;
                        continue;
                    }
                    _ => continue,
                };
                let parsed: WsMessage = match serde_json::from_str(&text) {
                    Ok(p) => p,
                    Err(e) => {
                        let _ = send_error(&mut writer, "INVALID_REQUEST", &e.to_string()).await;
                        continue;
                    }
                };
                if let Err(e) = dispatch(&mut writer, &bridge, parsed, &event_tx).await {
                    eprintln!("[mpe-bridge] 处理消息出错: {e}");
                    let _ = send_error(&mut writer, "INTERNAL_ERROR", &e).await;
                }
            }
        }
    }
    Ok(())
}

async fn dispatch<W>(
    writer: &mut W,
    bridge: &Bridge,
    msg: WsMessage,
    events: &mpsc::UnboundedSender<Message>,
) -> Result<(), String>
where
    W: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    match msg.path.as_str() {
        // 协议版本握手（MPE 较新版本会先发）。
        // MPE 的 HandshakeResponse 字段为 success/server_version/required_version/message。
        "/system/handshake" => {
            send_json(
                writer,
                "/system/handshake/response",
                json!({
                    "success": true,
                    "server_version": "1.0.3",
                    "required_version": "1.4.5",
                    "message": "ok",
                }),
            )
            .await
        }
        // 打开文件：file_path -> 返回解析后的 JSON 内容
        "/etl/open_file" => {
            let fp = msg
                .data
                .get("file_path")
                .and_then(|v| v.as_str())
                .ok_or("open_file 缺少 file_path")?;
            let path = safe_path(&bridge.root, fp)?;
            let content = read_json_file(&path)?;
            let fps = path.to_string_lossy().to_string();
            send_json(
                writer,
                "/lte/file_content",
                json!({ "file_path": fps, "content": content }),
            )
            .await
        }
        // 保存文件：file_path + content -> 写盘并回 ack
        "/etl/save_file" => {
            let fp = msg
                .data
                .get("file_path")
                .and_then(|v| v.as_str())
                .ok_or("save_file 缺少 file_path")?;
            let content = msg.data.get("content").ok_or("save_file 缺少 content")?;
            let path = safe_path(&bridge.root, fp)?;
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let text =
                serde_json::to_string_pretty(content).map_err(|e| format!("序列化失败: {e}"))?;
            std::fs::write(&path, text).map_err(|e| e.to_string())?;
            let fps = path.to_string_lossy().to_string();
            send_json(
                writer,
                "/ack/save_file",
                json!({ "file_path": fps, "status": "ok" }),
            )
            .await
        }
        // 创建文件：file_name + directory + content -> 写盘并重新推送文件列表
        "/etl/create_file" => {
            let file_name = msg
                .data
                .get("file_name")
                .and_then(|v| v.as_str())
                .ok_or("create_file 缺少 file_name")?;
            let directory = msg
                .data
                .get("directory")
                .and_then(|v| v.as_str())
                .ok_or("create_file 缺少 directory")?;
            let content = msg.data.get("content").cloned().unwrap_or(json!({}));
            let dir = safe_path(&bridge.root, directory)?;
            let name = sanitize_file_name(file_name);
            let path = dir.join(&name);
            if !path.starts_with(&bridge.root) {
                return Err("路径超出资源根目录".to_string());
            }
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let text = serde_json::to_string_pretty(&content).unwrap_or_else(|_| "{}".to_string());
            std::fs::write(&path, text).map_err(|e| e.to_string())?;
            // 创建后重新推送最新文件列表与资源包信息（LocalBridge 行为）
            push_file_state(writer, &bridge.root).await
        }
        // ---- MFW 控制器协议（Win32）：窗口枚举 / 连接 / 截图 / 断开 ----
        "/etl/mfw/refresh_win32_windows" => handle_refresh_win32_windows(writer).await,
        "/etl/mfw/create_win32_controller" => {
            handle_create_win32_controller(writer, &msg.data).await
        }
        "/etl/mfw/request_screencap" => handle_request_screencap(writer, &msg.data).await,
        "/etl/mfw/disconnect_controller" => handle_disconnect_controller(writer, &msg.data).await,
        // ---- MPE 调试协议：能力清单 / 资源检测 / 会话 / 运行 ----
        "/mpe/debug/capabilities" => {
            send_json(writer, "/lte/debug/capabilities", debug_capabilities()).await
        }
        "/mpe/debug/resource/preflight" => {
            handle_debug_resource_check(writer, &msg.data, "/lte/debug/resource_preflight").await
        }
        "/mpe/debug/resource/health" => {
            handle_debug_resource_check(writer, &msg.data, "/lte/debug/resource_health").await
        }
        "/mpe/debug/session/create" => handle_debug_session_create(writer).await,
        "/mpe/debug/session/destroy" => handle_debug_session_destroy(writer, &msg.data).await,
        "/mpe/debug/run/start" => handle_debug_run_start(writer, &msg.data, events).await,
        "/mpe/debug/run/stop" => handle_debug_run_stop(writer, &msg.data).await,
        // ---- 杂项：模板弹窗的图片列表 ----
        "/etl/get_image_list" => handle_get_image_list(writer, &bridge.root).await,
        other => {
            eprintln!("[mpe-bridge] 未处理的路由: {other}");
            Ok(())
        }
    }
}

/// 列举可自动化的桌面窗口，供 MPE 连接面板选择。
async fn handle_refresh_win32_windows<W>(writer: &mut W) -> Result<(), String>
where
    W: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let windows = match ensure_library_loaded() {
        Ok(_) => crate::device::list_windows().unwrap_or_default(),
        Err(e) => {
            eprintln!("[mpe-bridge] 窗口枚举失败：{e}");
            Vec::new()
        }
    };
    let list: Vec<Value> = windows
        .iter()
        .map(|w| {
            json!({
                "hwnd": w.hwnd.to_string(),
                "class_name": w.class_name,
                "window_name": w.window_name,
                // MPE 连接表单从这里取候选；Rust 侧统一用 All/Seize，忽略具体选项
                "screencap_methods": ["FramePool", "GDI", "DXGI_DesktopDup", "DXGI_DesktopDup_Window"],
                "input_methods": ["Seize"],
            })
        })
        .collect();
    send_json(writer, "/lte/mfw/win32_windows", json!({ "windows": list })).await
}

/// 解析 MPE 传来的窗口句柄（字符串或数字均可）。
fn parse_hwnd(data: &Value) -> Option<i64> {
    let raw = data.get("hwnd")?;
    if let Some(text) = raw.as_str() {
        return text.trim().parse::<i64>().ok();
    }
    raw.as_i64().or_else(|| raw.as_u64().map(|v| v as i64))
}

/// 创建 Win32 控制器并保持连接，回执 controller_created。
async fn handle_create_win32_controller<W>(writer: &mut W, data: &Value) -> Result<(), String>
where
    W: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let reply_error = |error: String| json!({ "success": false, "type": "win32", "error": error });

    let Some(hwnd) = parse_hwnd(data) else {
        return send_json(
            writer,
            "/lte/mfw/controller_created",
            reply_error("缺少有效的 hwnd".to_string()),
        )
        .await;
    };
    if let Err(e) = ensure_library_loaded() {
        return send_json(writer, "/lte/mfw/controller_created", reply_error(e)).await;
    }

    let hwnd_ptr = hwnd as *mut c_void;
    let controller = match Controller::new_win32(
        hwnd_ptr,
        WIN32_SCREENCAP_WINDOW,
        WIN32_INPUT_AUTO,
        WIN32_INPUT_AUTO,
    ) {
        Ok(controller) => controller,
        Err(e) => {
            return send_json(
                writer,
                "/lte/mfw/controller_created",
                reply_error(format!("创建 Win32 控制器失败：{e}")),
            )
            .await
        }
    };

    match controller.post_connection() {
        Ok(job) => {
            let _ = controller.wait(job);
        }
        Err(e) => {
            return send_json(
                writer,
                "/lte/mfw/controller_created",
                reply_error(format!("连接窗口失败：{e}")),
            )
            .await
        }
    }
    if !controller.connected() {
        return send_json(
            writer,
            "/lte/mfw/controller_created",
            reply_error(format!(
                "连接窗口 hwnd={hwnd} 失败：请确认窗口可见且未被最小化"
            )),
        )
        .await;
    }

    let controller_id = {
        let mut pool = controller_pool()
            .lock()
            .map_err(|_| "控制器池锁损坏".to_string())?;
        pool.next_id += 1;
        let id = format!("win32-{}", pool.next_id);
        pool.map.insert(id.clone(), MpeController { hwnd, controller });
        id
    };
    send_json(
        writer,
        "/lte/mfw/controller_created",
        json!({ "success": true, "controller_id": controller_id, "type": "win32" }),
    )
    .await
}

/// 截屏失败的统一回执。
fn screencap_failure(base: &Value, error: &str) -> Value {
    let mut payload = base.clone();
    payload["success"] = json!(false);
    payload["error"] = json!(error);
    payload
}

/// 执行一帧截屏并编码为 PNG data URL（阻塞 FFI，供 spawn_blocking 调用）。
fn do_screencap(controller: Controller, request_id: String, controller_id: String) -> Value {
    let base = json!({ "request_id": request_id, "controller_id": controller_id });
    if !controller.connected() {
        return screencap_failure(&base, "控制器未连接");
    }
    let job = match controller.post_screencap() {
        Ok(job) => job,
        Err(e) => return screencap_failure(&base, &format!("截屏失败：{e}")),
    };
    let _ = controller.wait(job);
    let image = match controller.cached_image() {
        Ok(image) => image,
        Err(_) => {
            return screencap_failure(
                &base,
                "截图失败：控制器缓存为空，请确认目标窗口可见且未被最小化",
            )
        }
    };
    match crate::capture::maa_image_png_bytes(&image) {
        Ok((bytes, width, height)) => {
            let mut payload = base;
            payload["success"] = json!(true);
            payload["image"] = json!(format!(
                "data:image/png;base64,{}",
                BASE64_STANDARD.encode(&bytes)
            ));
            payload["width"] = json!(width);
            payload["height"] = json!(height);
            payload
        }
        Err(e) => screencap_failure(&base, &e),
    }
}

/// 按 request_id 回一帧截图（MPE 实时画面 / 模板与 OCR 框选都依赖它）。
async fn handle_request_screencap<W>(writer: &mut W, data: &Value) -> Result<(), String>
where
    W: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let request_id = data
        .get("request_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let controller_id = data
        .get("controller_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let controller = controller_pool()
        .lock()
        .ok()
        .and_then(|pool| pool.map.get(&controller_id).map(|c| c.controller.clone()));
    let Some(controller) = controller else {
        return send_json(
            writer,
            "/lte/mfw/screencap_result",
            json!({
                "request_id": request_id,
                "controller_id": controller_id,
                "success": false,
                "error": "控制器不存在或已断开",
            }),
        )
        .await;
    };

    // MaaFramework 截屏是阻塞 FFI 调用，放到阻塞线程避免卡住异步运行时
    let request_id_blocking = request_id.clone();
    let controller_id_blocking = controller_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        do_screencap(controller, request_id_blocking, controller_id_blocking)
    })
    .await
    .unwrap_or_else(|e| {
        json!({
            "request_id": request_id,
            "controller_id": controller_id,
            "success": false,
            "error": e.to_string(),
        })
    });
    send_json(writer, "/lte/mfw/screencap_result", result).await
}

/// 断开并移除 MPE 侧控制器。
async fn handle_disconnect_controller<W>(writer: &mut W, data: &Value) -> Result<(), String>
where
    W: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let controller_id = data
        .get("controller_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if let Ok(mut pool) = controller_pool().lock() {
        pool.map.remove(&controller_id);
    }
    send_json(
        writer,
        "/lte/mfw/controller_status",
        json!({ "connected": false, "controller_id": controller_id }),
    )
    .await
}

/// 校验资源路径并产出诊断（调试预检与健康检查共用）。
async fn handle_debug_resource_check<W>(
    writer: &mut W,
    data: &Value,
    reply_route: &str,
) -> Result<(), String>
where
    W: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let started = std::time::Instant::now();
    let request_id = data
        .get("requestId")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let paths: Vec<String> = data
        .get("resourcePaths")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|p| p.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let mut diagnostics = Vec::new();
    for path in &paths {
        let dir = PathBuf::from(path);
        if !dir.is_dir() {
            diagnostics.push(json!({
                "severity": "error",
                "code": "debug.resource.missing",
                "message": format!("资源路径不存在或不是目录：{path}"),
            }));
        } else if !dir.join("pipeline").is_dir()
            && !dir.join("image").is_dir()
            && !dir.join("interface.json").is_dir()
        {
            diagnostics.push(json!({
                "severity": "warning",
                "code": "debug.resource.suspicious",
                "message": format!("目录未包含 pipeline/image/interface.json：{path}"),
            }));
        }
    }
    let has_error = diagnostics
        .iter()
        .any(|d| d["severity"] == json!("error"));

    let mut payload = json!({
        "resourcePaths": paths,
        "status": if has_error { "failed" } else { "ready" },
        "checkedAt": now_iso(),
        "durationMs": started.elapsed().as_millis() as u64,
        "diagnostics": diagnostics,
    });
    if let Some(rid) = request_id {
        payload["requestId"] = json!(rid);
    }
    send_json(writer, reply_route, payload).await
}

async fn handle_debug_session_create<W>(writer: &mut W) -> Result<(), String>
where
    W: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let session_id = format!(
        "dbg-session-{}",
        DEBUG_SESSION_SEQ.fetch_add(1, Ordering::Relaxed) + 1
    );
    send_json(
        writer,
        "/lte/debug/session_created",
        debug_session_snapshot(&session_id, "preparing"),
    )
    .await
}

async fn handle_debug_session_destroy<W>(writer: &mut W, data: &Value) -> Result<(), String>
where
    W: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let session_id = data
        .get("sessionId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if let Ok(mut state) = debug_run_state().lock() {
        *state = None;
    }
    send_json(
        writer,
        "/lte/debug/session_destroyed",
        json!({ "sessionId": session_id }),
    )
    .await
}

/// 从运行请求里解析要用的控制器：优先按 profile 里的 hwnd 匹配，否则取池中第一个。
fn find_pool_controller(hwnd: Option<i64>) -> Option<Controller> {
    let pool = controller_pool().lock().ok()?;
    match hwnd {
        Some(h) => pool
            .map
            .values()
            .find(|c| c.hwnd == h)
            .map(|c| c.controller.clone()),
        None => pool.map.values().next().map(|c| c.controller.clone()),
    }
}

/// 解析任意 JSON 值为 hwnd（字符串或数字）。
fn parse_hwnd_value(value: &Value) -> Option<i64> {
    if let Some(text) = value.as_str() {
        return text.trim().parse::<i64>().ok();
    }
    value
        .as_i64()
        .or_else(|| value.as_u64().map(|v| v as i64))
}

type DebugTaskJob = maa_framework::job::TaskJob<'static, maa_framework::common::TaskDetail>;

/// 调试运行的阻塞准备段：加载资源、应用节点覆盖、绑定控制器并提交任务。
fn prepare_debug_run(
    controller: Controller,
    resource_paths: Vec<String>,
    overrides: Value,
    entry: String,
) -> Result<(Tasker, DebugTaskJob), String> {
    ensure_library_loaded()?;
    if !controller.connected() {
        return Err("控制器未连接，请先在连接面板连接目标窗口".to_string());
    }

    let resource = Resource::new().map_err(|e| e.to_string())?;
    for path in &resource_paths {
        resource
            .post_bundle(path)
            .map_err(|e| format!("加载资源失败 {path}：{e}"))?;
    }
    // 资源加载是异步的，轮询等待（200 × 50ms = 最多 10 秒）
    let mut loaded = false;
    for _ in 0..200 {
        loaded = resource.loaded();
        if loaded {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    if !loaded {
        return Err(format!("资源加载超时：{:?}", resource_paths));
    }

    if overrides.as_object().map(|m| !m.is_empty()).unwrap_or(false) {
        resource
            .override_pipeline_json(&overrides)
            .map_err(|e| format!("应用节点覆盖失败：{e}"))?;
    }

    let tasker = Tasker::new().map_err(|e| e.to_string())?;
    tasker
        .bind(&resource, &controller)
        .map_err(|e| e.to_string())?;
    if !tasker.inited() {
        return Err("Tasker 初始化失败：资源或控制器未就绪".to_string());
    }
    let job = tasker
        .post_task(&entry, "{}")
        .map_err(|e| format!("提交任务失败：{e}"))?;
    Ok((tasker, job))
}

/// 启动调试运行：先回 run_started，再在后台等待任务完成并推送事件。
async fn handle_debug_run_start<W>(
    writer: &mut W,
    data: &Value,
    events: &mpsc::UnboundedSender<Message>,
) -> Result<(), String>
where
    W: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let session_id = data
        .get("sessionId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let mode = data
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let profile = data.get("profile").cloned().unwrap_or(json!({}));

    let entry = data
        .get("target")
        .and_then(|t| t.get("runtimeName"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            profile
                .get("entry")
                .and_then(|e| e.get("runtimeName"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        });
    let Some(entry) = entry else {
        return send_json(
            writer,
            "/lte/debug/error",
            json!({ "code": "debug.run.no_entry", "message": "运行请求缺少入口节点" }),
        )
        .await;
    };

    let resource_paths: Vec<String> = profile
        .get("resourcePaths")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|p| p.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    if resource_paths.is_empty() {
        return send_json(
            writer,
            "/lte/debug/error",
            json!({ "code": "debug.run.no_resource", "message": "运行请求未携带资源路径" }),
        )
        .await;
    }

    // overrides: [{runtimeName, pipeline}] -> { runtimeName: pipeline }
    let mut override_map = serde_json::Map::new();
    if let Some(list) = data.get("overrides").and_then(|v| v.as_array()) {
        for item in list {
            if let (Some(name), Some(pipeline)) = (
                item.get("runtimeName").and_then(|v| v.as_str()),
                item.get("pipeline"),
            ) {
                override_map.insert(name.to_string(), pipeline.clone());
            }
        }
    }

    let hwnd = profile
        .get("controller")
        .and_then(|c| c.get("options"))
        .and_then(|o| o.get("hwnd"))
        .and_then(parse_hwnd_value);
    let Some(controller) = find_pool_controller(hwnd) else {
        return send_json(
            writer,
            "/lte/debug/error",
            json!({ "code": "debug.run.no_controller", "message": "请先在连接面板连接目标窗口" }),
        )
        .await;
    };

    // 阻塞准备段放阻塞线程，避免卡住异步运行时
    let entry_for_run = entry.clone();
    let prepared = tauri::async_runtime::spawn_blocking(move || {
        prepare_debug_run(controller, resource_paths, Value::Object(override_map), entry_for_run)
    })
    .await
    .map_err(|e| e.to_string())?;

    let (tasker, job) = match prepared {
        Ok(ok) => ok,
        Err(e) => {
            return send_json(
                writer,
                "/lte/debug/error",
                json!({ "code": "debug.run.failed", "message": e }),
            )
            .await
        }
    };

    let run_id = format!("dbg-run-{}", DEBUG_RUN_SEQ.fetch_add(1, Ordering::Relaxed) + 1);
    {
        let mut state = debug_run_state()
            .lock()
            .map_err(|_| "调试状态锁损坏".to_string())?;
        *state = Some(DebugRunState {
            run_id: run_id.clone(),
            tasker: tasker.clone(),
        });
    }

    send_json(
        writer,
        "/lte/debug/run_started",
        json!({
            "sessionId": session_id,
            "runId": run_id,
            "mode": mode,
            "entry": entry,
            "startedAt": now_iso(),
            "session": debug_session_snapshot(&session_id, "running"),
        }),
    )
    .await?;

    // 后台等待任务完成，把结果事件推回客户端
    let events = events.clone();
    let session_id_bg = session_id.clone();
    let run_id_bg = run_id.clone();
    tauri::async_runtime::spawn(async move {
        let status = tauri::async_runtime::spawn_blocking(move || job.wait())
            .await
            .unwrap_or(MaaStatus::FAILED);
        let phase = if status == MaaStatus::SUCCEEDED {
            "completed"
        } else {
            "failed"
        };
        let _ = events.send(message_for(
            "/lte/debug/event",
            json!({
                "sessionId": session_id_bg,
                "runId": run_id_bg,
                "seq": 1,
                "timestamp": now_iso(),
                "source": "maafw",
                "kind": "task",
                "phase": phase,
            }),
        ));
        let _ = events.send(message_for(
            "/lte/debug/session_snapshot",
            debug_session_snapshot(&session_id_bg, if phase == "completed" { "completed" } else { "failed" }),
        ));
        if let Ok(mut state) = debug_run_state().lock() {
            *state = None;
        }
    });
    Ok(())
}

async fn handle_debug_run_stop<W>(writer: &mut W, data: &Value) -> Result<(), String>
where
    W: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let session_id = data
        .get("sessionId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let stopped = debug_run_state().lock().ok().and_then(|state| {
        state
            .as_ref()
            .map(|run| {
                let _ = run.tasker.post_stop();
                run.run_id.clone()
            })
    });
    match stopped {
        Some(run_id) => {
            send_json(
                writer,
                "/lte/debug/run_stop_requested",
                json!({ "sessionId": session_id, "runId": run_id, "reason": "用户请求停止" }),
            )
            .await
        }
        None => {
            send_json(
                writer,
                "/lte/debug/error",
                json!({ "code": "debug.stop.no_run", "message": "当前没有正在运行的调试任务" }),
            )
            .await
        }
    }
}

/// 递归收集 bundle image 目录下的图片（供模板弹窗的图片列表）。
fn collect_images(dir: &Path, root: &Path, bundle: &str, out: &mut Vec<Value>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().map(|n| n == "image").unwrap_or(false) {
                collect_image_files(&path, root, bundle, out);
            } else {
                collect_images(&path, root, bundle, out);
            }
        }
    }
}

fn collect_image_files(dir: &Path, root: &Path, bundle: &str, out: &mut Vec<Value>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_image_files(&path, root, bundle, out);
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("png") | Some("jpg") | Some("jpeg") | Some("bmp") | Some("webp")
        ) {
            if let Some(rel) = path.strip_prefix(root).ok().and_then(|p| p.to_str()) {
                out.push(json!({
                    "relative_path": rel,
                    "bundle_name": bundle,
                    "file_name": path.file_name().unwrap_or_default().to_string_lossy(),
                }));
            }
        }
    }
}

async fn handle_get_image_list<W>(writer: &mut W, root: &Path) -> Result<(), String>
where
    W: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let mut images = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let bundle = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                collect_images(&path, root, &bundle, &mut images);
            }
        }
    }
    send_json(
        writer,
        "/lte/image_list",
        json!({ "images": images, "bundle_name": "", "is_filtered": false }),
    )
    .await
}

/// 递归收集 MPE 关心的文件：各 bundle 的 `pipeline/**.json(c)` 与根目录的 `interface.json`。
fn build_file_list(root: &Path) -> Vec<Value> {
    let mut out = Vec::new();
    collect_files(root, root, &mut out);
    out
}

fn collect_files(dir: &Path, root: &Path, out: &mut Vec<Value>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, root, out);
        } else if path.is_file() {
            let in_pipeline = path.components().any(|c| c.as_os_str() == "pipeline");
            let is_json = matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("json") | Some("jsonc")
            );
            let is_interface = path
                .file_name()
                .map(|n| n == "interface.json")
                .unwrap_or(false)
                && path.parent() == Some(root);
            if (in_pipeline && is_json) || is_interface {
                if let Some(rel) = path.strip_prefix(root).ok().and_then(|p| p.to_str()) {
                    let bundle_name = if is_interface {
                        String::new()
                    } else {
                        path.strip_prefix(root)
                            .ok()
                            .and_then(|p| p.components().next())
                            .map(|c| c.as_os_str().to_string_lossy().to_string())
                            .unwrap_or_default()
                    };
                    let prefix = path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let last_modified = std::fs::metadata(&path)
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_nanos() as u64)
                        .unwrap_or(0);
                    out.push(json!({
                        "file_path": path.to_string_lossy(),
                        "file_name": path.file_name().unwrap_or_default().to_string_lossy(),
                        "relative_path": rel,
                        "bundle_name": bundle_name,
                        "prefix": prefix,
                        "nodes": [],
                        "last_modified": last_modified,
                    }));
                }
            }
        }
    }
}

/// 扫描根目录下的一级子目录，作为 MPE 的资源包信息推送。
fn build_resource_bundles(root: &Path) -> Vec<Value> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let rel = path
            .strip_prefix(root)
            .ok()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_string();
        let pipeline_dir = path.join("pipeline");
        let image_dir = path.join("image");
        let model_dir = path.join("model");
        let default_pipeline = path.join("default_pipeline.json");
        let has_image = image_dir.exists();
        out.push(json!({
            "abs_path": path.to_string_lossy(),
            "rel_path": rel,
            "name": name,
            "has_pipeline": pipeline_dir.exists(),
            "has_image": has_image,
            "has_model": model_dir.exists(),
            "has_default_pipeline": default_pipeline.exists(),
            "image_dir": if has_image { image_dir.to_string_lossy().to_string() } else { "".to_string() },
        }));
    }
    out
}

fn build_image_dirs(bundles: &[Value]) -> Vec<Value> {
    bundles
        .iter()
        .filter_map(|b| {
            let dir = b["image_dir"].as_str()?;
            if dir.is_empty() {
                None
            } else {
                Some(Value::String(dir.to_string()))
            }
        })
        .collect()
}

/// 向客户端推送最新的文件列表与资源包信息（连接建立、创建文件后使用）。
async fn push_file_state<W>(writer: &mut W, root: &Path) -> Result<(), String>
where
    W: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let list = build_file_list(root);
    let bundles = build_resource_bundles(root);
    let image_dirs = build_image_dirs(&bundles);
    send_json(
        writer,
        "/lte/file_list",
        json!({
            "root": root.to_string_lossy(),
            "files": list,
        }),
    )
    .await?;
    send_json(
        writer,
        "/lte/resource_bundles",
        json!({
            "root": root.to_string_lossy(),
            "bundles": bundles,
            "image_dirs": image_dirs,
        }),
    )
    .await
}

/// 把任意（可能带绝对路径 / `..` 的）路径限制在 root 之内，防止目录穿越。
fn safe_path(root: &Path, raw: &str) -> Result<PathBuf, String> {
    // Web 端常传正斜杠路径，统一成本地分隔符，保证与 root 比较一致
    let normalized_raw = raw.replace('/', std::path::MAIN_SEPARATOR_STR);
    let base: PathBuf = if Path::new(&normalized_raw).is_absolute() {
        PathBuf::from(&normalized_raw)
    } else {
        root.join(&normalized_raw)
    };
    let mut normalized = PathBuf::new();
    for comp in base.components() {
        match comp {
            Component::ParentDir => return Err("路径包含非法父目录".to_string()),
            Component::CurDir => {}
            other => normalized.push(other.as_os_str()),
        }
    }
    if !normalized.starts_with(root) {
        return Err("路径超出资源根目录".to_string());
    }
    Ok(normalized)
}

/// 规整文件名：去分隔符、补 .json 扩展名，避免把整目录结构写炸。
fn sanitize_file_name(name: &str) -> String {
    let mut n = name.replace(['/', '\\'], "");
    if n.is_empty() {
        n = "untitled.json".to_string();
    }
    if !n.ends_with(".json") && !n.ends_with(".jsonc") {
        n.push_str(".json");
    }
    n
}

/// 读取并解析 JSON / JSONC 文件（MPE 用 .jsonc 也常见，故用 json5 解析注释）。
fn read_json_file(path: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("读取失败: {e}"))?;
    json5::from_str(&text).map_err(|e| format!("JSON 解析失败: {e}"))
}

async fn send_json<W>(writer: &mut W, path: &str, data: Value) -> Result<(), String>
where
    W: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let msg = json!({ "path": path, "data": data });
    let text = serde_json::to_string(&msg).map_err(|e| e.to_string())?;
    writer
        .send(Message::text(text))
        .await
        .map_err(|e| e.to_string())
}

async fn send_error<W>(writer: &mut W, code: &str, message: &str) -> Result<(), String>
where
    W: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    send_json(
        writer,
        "/error",
        json!({ "code": code, "message": message }),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::Sink;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tokio_tungstenite::tungstenite::Message;

    /// 捕获所有发出消息的内存 Sink，用于在不启动真实 WS 服务的情况下验证 dispatch 协议行为。
    struct CaptureSink {
        items: Vec<Message>,
    }

    impl Sink<Message> for CaptureSink {
        type Error = tokio_tungstenite::tungstenite::Error;

        fn poll_ready(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn start_send(self: Pin<&mut Self>, item: Message) -> Result<(), Self::Error> {
            self.get_mut().items.push(item);
            Ok(())
        }

        fn poll_flush(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn poll_close(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
    }

    // ---------- safe_path：目录穿越防护 ----------
    #[test]
    fn safe_path_accepts_relative_under_root() {
        let root = PathBuf::from("C:\\proj\\resource");
        let p = safe_path(&root, "MyBundle/pipeline/x.json").unwrap();
        assert!(p.starts_with(&root));
        assert!(p.ends_with("x.json"));
    }

    #[test]
    fn safe_path_rejects_dotdot_escape() {
        let root = PathBuf::from("C:\\proj\\resource");
        assert!(safe_path(&root, "../secret.json").is_err());
        assert!(safe_path(&root, "MyBundle/../../etc/passwd").is_err());
        assert!(safe_path(&root, "..\\..\\win.ini").is_err());
    }

    #[test]
    fn safe_path_rejects_absolute_outside_root() {
        let root = PathBuf::from("C:\\proj\\resource");
        // 跨盘符
        assert!(safe_path(&root, "D:\\evil\\x.json").is_err());
        // 绝对路径 + .. 逃出后再回来
        assert!(safe_path(&root, "C:\\proj\\resource\\..\\outside.json").is_err());
    }

    #[test]
    fn safe_path_normalizes_separators() {
        let root = PathBuf::from("C:\\proj\\resource");
        let p = safe_path(&root, "MyBundle/pipeline/sub/y.json").unwrap();
        assert!(p.to_string_lossy().contains("pipeline"));
        assert!(p.to_string_lossy().contains("y.json"));
    }

    // ---------- sanitize_file_name ----------
    #[test]
    fn sanitize_file_name_behaves() {
        assert_eq!(sanitize_file_name("foo"), "foo.json");
        assert_eq!(sanitize_file_name("a/b\\c"), "abc.json");
        assert_eq!(sanitize_file_name("x.jsonc"), "x.jsonc");
        assert_eq!(sanitize_file_name(""), "untitled.json");
    }

    // ---------- build_file_list：只列 pipeline + interface ----------
    #[test]
    fn file_list_only_pipeline_and_interface() {
        let tmp = std::env::temp_dir().join(format!("mpe_list_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("BundleA/pipeline")).unwrap();
        std::fs::create_dir_all(tmp.join("BundleA/image")).unwrap();
        std::fs::write(tmp.join("BundleA/pipeline/a.json"), "{}").unwrap();
        std::fs::write(tmp.join("BundleA/pipeline/b.jsonc"), "{}").unwrap();
        std::fs::write(tmp.join("interface.json"), "{}").unwrap();
        std::fs::write(tmp.join("BundleA/readme.txt"), "x").unwrap();
        std::fs::write(tmp.join("BundleA/image/icon.png"), "x").unwrap();

        let list = build_file_list(&tmp);
        let names: Vec<String> = list
            .iter()
            .map(|v| v["file_name"].as_str().unwrap().to_string())
            .collect();
        assert!(names.contains(&"a.json".to_string()));
        assert!(names.contains(&"b.jsonc".to_string()));
        assert!(names.contains(&"interface.json".to_string()));
        assert!(!names.iter().any(|n| n == "readme.txt"));
        assert!(!names.iter().any(|n| n == "icon.png"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ---------- read_json_file 支持 jsonc ----------
    #[test]
    fn read_json_file_parses_comments() {
        let tmp = std::env::temp_dir().join("mpe_jsonc_test.jsonc");
        std::fs::write(
            &tmp,
            "// leading comment\n{ \"a\": 1, /* inline */ \"b\": 2 }",
        )
        .unwrap();
        let v = read_json_file(&tmp).unwrap();
        assert_eq!(v["a"], 1);
        assert_eq!(v["b"], 2);
        let _ = std::fs::remove_file(&tmp);
    }

    // ---------- dispatch 协议行为（内存 Sink） ----------
    #[tokio::test]
    async fn dispatch_open_traversal_is_rejected() {
        let root = std::env::temp_dir().join(format!("mpe_disp_{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let bridge = Bridge { root: root.clone() };
        let mut sink = CaptureSink { items: Vec::new() };
        let (event_tx, _event_rx) = mpsc::unbounded_channel::<Message>();
        let res = dispatch(
            &mut sink,
            &bridge,
            WsMessage {
                path: "/etl/open_file".to_string(),
                data: json!({ "file_path": "../secret.json" }),
            },
            &event_tx,
        )
        .await;
        assert!(res.is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn dispatch_save_then_open_roundtrip() {
        let root = std::env::temp_dir().join(format!("mpe_disp2_{}", std::process::id()));
        std::fs::create_dir_all(root.join("BundleA/pipeline")).unwrap();
        let bridge = Bridge { root: root.clone() };
        let mut sink = CaptureSink { items: Vec::new() };
        let (event_tx, _event_rx) = mpsc::unbounded_channel::<Message>();
        let fp = root
            .join("BundleA/pipeline/new.json")
            .to_string_lossy()
            .to_string();

        // 保存
        dispatch(
            &mut sink,
            &bridge,
            WsMessage {
                path: "/etl/save_file".to_string(),
                data: json!({ "file_path": fp, "content": { "name": "n", "recognition": "Auto" } }),
            },
            &event_tx,
        )
        .await
        .unwrap();
        let last = sink
            .items
            .last()
            .unwrap()
            .to_text()
            .unwrap()
            .to_string();
        assert!(last.contains("/ack/save_file"));

        // 打开
        sink.items.clear();
        dispatch(
            &mut sink,
            &bridge,
            WsMessage {
                path: "/etl/open_file".to_string(),
                data: json!({ "file_path": fp }),
            },
            &event_tx,
        )
        .await
        .unwrap();
        let resp: Value = serde_json::from_str(sink.items[0].to_text().unwrap()).unwrap();
        assert_eq!(resp["path"], "/lte/file_content");
        assert_eq!(resp["data"]["content"]["name"], "n");

        let _ = std::fs::remove_dir_all(&root);
    }

    // ---------- MFW 控制器协议 ----------
    #[tokio::test]
    async fn dispatch_refresh_win32_windows_replies_list() {
        let root = std::env::temp_dir().join(format!("mpe_wins_{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let bridge = Bridge { root };
        let mut sink = CaptureSink { items: Vec::new() };
        let (event_tx, _event_rx) = mpsc::unbounded_channel::<Message>();
        dispatch(
            &mut sink,
            &bridge,
            WsMessage {
                path: "/etl/mfw/refresh_win32_windows".to_string(),
                data: json!({}),
            },
            &event_tx,
        )
        .await
        .unwrap();
        let first = sink.items.first().expect("应回窗口列表");
        let resp: Value = serde_json::from_str(first.to_text().unwrap()).unwrap();
        assert_eq!(resp["path"], "/lte/mfw/win32_windows");
        assert!(resp["data"]["windows"].is_array());
    }

    #[tokio::test]
    async fn dispatch_screencap_without_controller_reports_failure() {
        let root = std::env::temp_dir().join(format!("mpe_cap_{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let bridge = Bridge { root };
        let mut sink = CaptureSink { items: Vec::new() };
        let (event_tx, _event_rx) = mpsc::unbounded_channel::<Message>();
        dispatch(
            &mut sink,
            &bridge,
            WsMessage {
                path: "/etl/mfw/request_screencap".to_string(),
                data: json!({ "controller_id": "nope", "request_id": "r1" }),
            },
            &event_tx,
        )
        .await
        .unwrap();
        let resp: Value = serde_json::from_str(sink.items[0].to_text().unwrap()).unwrap();
        assert_eq!(resp["path"], "/lte/mfw/screencap_result");
        assert_eq!(resp["data"]["success"], false);
        assert_eq!(resp["data"]["request_id"], "r1");
    }

    #[tokio::test]
    async fn dispatch_resource_preflight_reports_ready() {
        let root = std::env::temp_dir().join(format!("mpe_pf_{}", std::process::id()));
        std::fs::create_dir_all(root.join("BundleA/pipeline")).unwrap();
        let bridge = Bridge { root: root.clone() };
        let mut sink = CaptureSink { items: Vec::new() };
        let (event_tx, _event_rx) = mpsc::unbounded_channel::<Message>();
        let bundle = root.join("BundleA").to_string_lossy().to_string();
        dispatch(
            &mut sink,
            &bridge,
            WsMessage {
                path: "/mpe/debug/resource/preflight".to_string(),
                data: json!({ "resourcePaths": [bundle] }),
            },
            &event_tx,
        )
        .await
        .unwrap();
        let resp: Value = serde_json::from_str(sink.items[0].to_text().unwrap()).unwrap();
        assert_eq!(resp["path"], "/lte/debug/resource_preflight");
        assert_eq!(resp["data"]["status"], "ready");
        let _ = std::fs::remove_dir_all(&root);
    }

    // ---------- 集成测试：真实 TCP + WebSocket 端到端 ----------
    #[tokio::test]
    async fn bridge_ws_end_to_end() {
        use tokio_tungstenite::connect_async;

        // 准备临时资源目录
        let root = std::env::temp_dir().join(format!("mpe_e2e_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("BundleA/pipeline")).unwrap();
        std::fs::write(root.join("BundleA/pipeline/init.json"), r#"{ "a": 1 }"#).unwrap();
        std::fs::write(root.join("interface.json"), r#"{}"#).unwrap();

        // 绑定随机端口，避免与 9066 占用冲突
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(run_server_with_listener(listener, root.clone()));

        // 连接 WS
        let url = format!("ws://127.0.0.1:{}", port);
        let (mut ws, _) = connect_async(&url)
            .await
            .expect("应能连到桥");

        // 1. 连接建立后先收到 /lte/file_list 与 /lte/resource_bundles
        let msg = ws.next().await.unwrap().unwrap();
        let init: Value = serde_json::from_str(msg.to_text().unwrap()).unwrap();
        assert_eq!(init["path"], "/lte/file_list");
        assert!(!init["data"]["files"].as_array().unwrap().is_empty());
        let file = &init["data"]["files"][0];
        assert!(file["bundle_name"].is_string());
        assert!(file["prefix"].is_string());

        let msg = ws.next().await.unwrap().unwrap();
        let bundles_msg: Value = serde_json::from_str(msg.to_text().unwrap()).unwrap();
        assert_eq!(bundles_msg["path"], "/lte/resource_bundles");
        assert!(bundles_msg["data"]["bundles"].is_array());

        // 2. 发送握手
        ws.send(Message::Text(
            json!({ "path": "/system/handshake", "data": { "protocol_version": "1.4.5" } }).to_string(),
        ))
        .await
        .unwrap();
        let resp = ws.next().await.unwrap().unwrap();
        let resp: Value = serde_json::from_str(resp.to_text().unwrap()).unwrap();
        assert_eq!(resp["path"], "/system/handshake/response");
        assert_eq!(resp["data"]["success"], true);

        // 3. 打开文件
        let fp = root.join("BundleA/pipeline/init.json").to_string_lossy().to_string();
        ws.send(Message::Text(
            json!({ "path": "/etl/open_file", "data": { "file_path": fp } }).to_string(),
        ))
        .await
        .unwrap();
        let resp = ws.next().await.unwrap().unwrap();
        let resp: Value = serde_json::from_str(resp.to_text().unwrap()).unwrap();
        assert_eq!(resp["path"], "/lte/file_content");
        assert_eq!(resp["data"]["content"]["a"], 1);

        // 4. 保存文件
        ws.send(Message::Text(
            json!({ "path": "/etl/save_file", "data": { "file_path": fp, "content": { "a": 2 } } }).to_string(),
        ))
        .await
        .unwrap();
        let resp = ws.next().await.unwrap().unwrap();
        let resp: Value = serde_json::from_str(resp.to_text().unwrap()).unwrap();
        assert_eq!(resp["path"], "/ack/save_file");
        assert_eq!(resp["data"]["status"], "ok");

        // 5. 磁盘确认
        let text = std::fs::read_to_string(&root.join("BundleA/pipeline/init.json")).unwrap();
        assert!(text.contains("\"a\": 2"));

        // 清理
        let _ = std::fs::remove_dir_all(&root);
        let _ = server.abort();
    }
}
