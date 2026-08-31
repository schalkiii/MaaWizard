mod ai;
mod capture;
use tauri::Manager;
mod commands;
mod device;
mod maa;
mod pipeline;
mod recorder;

/// 启动 Tauri 应用：注册全部运行时状态与命令
pub fn run() {
    tauri::Builder::default()
        .manage(maa::MaaRuntime::default())
        .manage(pipeline::PipelineState::default())
        .manage(recorder::RecorderState::default())
        .setup(|app| {
            // 启动时尝试自动加载 MaaFramework 动态库，避免用户点「刷新窗口」等操作时
            // 因库未加载导致 FFI 空指针段错误（进程直接闪退）。加载失败仅告警，不阻断启动。
            let runtime = app.state::<maa::MaaRuntime>();
            let dll = maa::resolve_existing_path("maa-sdk/bin/MaaFramework.dll");
            if dll.exists() {
                match runtime.load_library(dll.to_str().unwrap_or("")) {
                    Ok(msg) => eprintln!("[maa] {msg}"),
                    Err(e) => eprintln!("[maa] 自动加载 MaaFramework 失败：{e}"),
                }
            } else {
                eprintln!(
                    "[maa] 未找到 {}，请在界面手动『加载动态库』（make fetch-sdk 下载）",
                    dll.display()
                );
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // M0 运行时
            commands::maa_load_library,
            commands::maa_find_adb_devices,
            commands::maa_connect_adb,
            commands::maa_connect_win32,
            commands::maa_load_resource,
            commands::maa_run_task,
            commands::maa_stop,
            commands::maa_status,
            commands::maa_controller_screenshot,
            // M1 图编辑器
            commands::pipeline_open,
            commands::pipeline_save,
            commands::pipeline_get,
            commands::pipeline_update_node,
            commands::pipeline_add_node,
            commands::pipeline_delete_node,
            commands::pipeline_validate,
            // M2/M3 录制与模板抓取
            commands::recorder_start,
            commands::recorder_stop,
            commands::recorder_status,
            commands::recorder_commit,
            commands::capture_grab_template,
            commands::capture_screenshot,
            // M4 设备管理
            commands::device_list_windows,
            // M6 AI 增强
            commands::ai_detect,
            commands::ai_run,
        ])
        .run(tauri::generate_context!())
        .expect("启动 Tauri 应用失败");
}
