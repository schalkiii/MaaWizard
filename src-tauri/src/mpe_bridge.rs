//! MaaPipelineEditor (MPE) LocalBridge 兼容文件桥。
//!
//! MPE 前端是纯静态网页（React + React Flow），默认通过 WebSocket 连接
//! `ws://localhost:9066` 与本地"桥"通信，用来读写本地 pipeline 文件。
//! 官方 LocalBridge（Go）目前只实现了「文件管理」这一类协议（列目录 / 打开 /
//! 保存 / 创建），截图、控制器连接等尚未实现（由我们自己的运行页负责）。
//!
//! 这里用 Rust 在 Tauri 进程内起一个等价的 WS 服务，根目录指向 `resource/`，
//! 让内嵌的 MPE 前端能直接编辑我们资源包里的 pipeline JSON。

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

use crate::maa::resolve_existing_path;

/// LocalBridge 默认端口，与 MPE 前端硬编码的 `ws://localhost:9066` 一致。
pub const BRIDGE_PORT: u16 = 9066;

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
    let root = resolve_existing_path("resource");
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

    // 连接建立后先推送当前文件列表与资源包列表（LocalBridge 行为）。
    push_file_state(&mut writer, &bridge.root).await?;

    while let Some(msg) = reader.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(_) => break,
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
        if let Err(e) = dispatch(&mut writer, &bridge, parsed).await {
            eprintln!("[mpe-bridge] 处理消息出错: {e}");
            let _ = send_error(&mut writer, "INTERNAL_ERROR", &e).await;
        }
    }
    Ok(())
}

async fn dispatch<W>(writer: &mut W, bridge: &Bridge, msg: WsMessage) -> Result<(), String>
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
        other => {
            eprintln!("[mpe-bridge] 未处理的路由: {other}");
            Ok(())
        }
    }
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
        let res = dispatch(
            &mut sink,
            &bridge,
            WsMessage {
                path: "/etl/open_file".to_string(),
                data: json!({ "file_path": "../secret.json" }),
            },
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
        )
        .await
        .unwrap();
        let resp: Value = serde_json::from_str(sink.items[0].to_text().unwrap()).unwrap();
        assert_eq!(resp["path"], "/lte/file_content");
        assert_eq!(resp["data"]["content"]["name"], "n");

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
