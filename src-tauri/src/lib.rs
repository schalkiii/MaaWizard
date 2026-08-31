mod ai;
mod capture;
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
