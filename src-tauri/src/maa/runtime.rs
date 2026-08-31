use std::os::raw::c_void;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use maa_framework::controller::Controller;
use maa_framework::resource::Resource;
use maa_framework::sys::{
    MaaWin32InputMethod as Win32InputMethod, MaaWin32ScreencapMethod as Win32ScreencapMethod,
};
use maa_framework::tasker::Tasker;
use maa_framework::toolkit::Toolkit;
use serde::Serialize;

/// 传 0 表示由 MaaFramework 自行选择 Win32 的截图/键鼠实现方式
const WIN32_SCREENCAP_AUTO: Win32ScreencapMethod = 0;
const WIN32_INPUT_AUTO: Win32InputMethod = 0;

/// 资源加载为异步操作，这里轮询等待其完成：200 × 50ms = 最多 10 秒
const RESOURCE_LOAD_MAX_POLL: u32 = 200;
const RESOURCE_LOAD_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Serialize)]
pub struct AdbDeviceInfo {
    pub adb_path: String,
    pub address: String,
    pub config: String,
}

/// Maa 运行时持有的全部可变状态。
/// Controller / Resource / Tasker 均实现了 Send + Sync，可安全置于 Tauri 状态中。
#[derive(Default)]
struct RuntimeInner {
    library_loaded: bool,
    resource: Option<Resource>,
    controller: Option<Controller>,
    tasker: Option<Tasker>,
}

/// M0：对 maa-framework-rs 的薄封装，是其它模块访问 MaaFramework 的唯一入口。
/// 所有 Maa 对象都存活在 Rust 侧（见 ADR 0002：后端为唯一真相源）。
pub struct MaaRuntime {
    inner: Mutex<RuntimeInner>,
}

/// 把 DLL 所在目录加入 Windows 的 DLL 搜索路径。
/// 非 Windows 平台为空实现（本项目只支持 Win32 控制器，接口保持一致即可）。
#[cfg(windows)]
fn prepare_dll_search_path(dll_path: &Path) {
    use std::os::windows::ffi::OsStrExt;

    let Some(directory) = dll_path.parent() else {
        return;
    };
    if directory.as_os_str().is_empty() {
        return;
    }

    let wide: Vec<u16> = directory
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    // SetDirectoryW 影响后续所有 LoadLibrary 调用，正是这里需要的
    unsafe {
        SetDllDirectoryW(wide.as_ptr());
    }
}

#[cfg(not(windows))]
fn prepare_dll_search_path(_dll_path: &Path) {}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn SetDllDirectoryW(path: *const u16) -> i32;
}

impl Default for MaaRuntime {
    fn default() -> Self {
        Self {
            inner: Mutex::new(RuntimeInner::default()),
        }
    }
}

impl MaaRuntime {
    /// 加载 MaaFramework 动态库并初始化全局选项。必须先于任何其它 API 调用。
    pub fn load_library(&self, dll_path: &str) -> Result<String, String> {
        let mut inner = self.lock()?;

        // dynamic 链接模式下必须显式加载；该 feature 属于依赖，不能用 cfg(feature) 判断
        let resolved = resolve_existing_path(dll_path);
        if !resolved.exists() {
            return Err(format!(
                "找不到动态库：{}（请确认 maa-sdk 已下载，或填写绝对路径）",
                resolved.display()
            ));
        }

        // MaaFramework.dll 依赖同目录的 MaaUtils / opencv / onnxruntime 等 DLL，
        // 而 Windows 加载 DLL 时不会搜索被加载 DLL 自身的目录，必须先把它加进搜索路径，
        // 否则会报 LoadLibraryExW failed。
        prepare_dll_search_path(&resolved);

        maa_framework::load_library(&resolved).map_err(|e| {
            format!(
                "加载 {} 失败：{}（该 DLL 依赖同目录的其他 DLL，请确认 maa-sdk/bin 完整且架构匹配）",
                resolved.display(),
                e
            )
        })?;

        // 用户数据放在可执行文件旁边，避免 dev 模式下污染源码目录
        let user_path = current_exe_dir().join("maa_userdata");
        std::fs::create_dir_all(&user_path).map_err(|e| e.to_string())?;
        Toolkit::init_option(user_path.to_str().unwrap_or("."), "{}").map_err(|e| e.to_string())?;

        inner.library_loaded = true;
        Ok(format!(
            "MaaFramework v{} 已加载：{}",
            maa_framework::maa_version(),
            resolved.display()
        ))
    }

    /// 扫描已连接的 ADB 设备（含常见模拟器）
    pub fn find_adb_devices(&self) -> Result<Vec<AdbDeviceInfo>, String> {
        self.ensure_loaded()?;
        let devices = Toolkit::find_adb_devices().map_err(|e| e.to_string())?;
        Ok(devices
            .iter()
            .map(|device| AdbDeviceInfo {
                adb_path: device.adb_path.to_string_lossy().to_string(),
                address: device.address.to_string(),
                config: device.config.to_string(),
            })
            .collect())
    }

    /// 连接 ADB 设备（Android 模拟器或真机）
    pub fn connect_adb(&self, adb_path: &str, address: &str, config: &str) -> Result<String, String> {
        self.ensure_loaded()?;
        let controller =
            Controller::new_adb(adb_path, address, config, "").map_err(|e| e.to_string())?;
        self.attach_controller(controller, format!("ADB {}", address))
    }

    /// 连接 Win32 窗口（Windows 桌面应用自动化）
    pub fn connect_win32(&self, hwnd: i64) -> Result<String, String> {
        self.ensure_loaded()?;
        let hwnd_ptr = hwnd as *mut c_void;
        let controller =
            Controller::new_win32(hwnd_ptr, WIN32_SCREENCAP_AUTO, WIN32_INPUT_AUTO, WIN32_INPUT_AUTO)
                .map_err(|e| e.to_string())?;
        self.attach_controller(controller, format!("Win32 hwnd={}", hwnd))
    }

    /// 加载资源包（Bundle），并等待其异步加载完成
    pub fn load_resource(&self, path: &str) -> Result<String, String> {
        self.ensure_loaded()?;
        let mut inner = self.lock()?;

        let resource = Resource::new().map_err(|e| e.to_string())?;
        let tasker = Tasker::new().map_err(|e| e.to_string())?;

        // 资源加载是异步的：先完成绑定，再发起加载并轮询等待
        match &inner.controller {
            Some(controller) => tasker.bind(&resource, controller).map_err(|e| e.to_string())?,
            None => tasker.bind_resource(&resource).map_err(|e| e.to_string())?,
        }

        let resolved = resolve_existing_path(path);
        resource
            .post_bundle(&resolved.to_string_lossy())
            .map_err(|e| e.to_string())?;

        let mut loaded = false;
        for _ in 0..RESOURCE_LOAD_MAX_POLL {
            loaded = tasker.resource().map(|res| res.loaded()).unwrap_or(false);
            if loaded {
                break;
            }
            std::thread::sleep(RESOURCE_LOAD_POLL_INTERVAL);
        }

        inner.resource = Some(resource);
        inner.tasker = Some(tasker);

        if !loaded {
            return Err(format!("资源加载超时：{}", resolved.display()));
        }
        Ok(format!("资源已加载：{}", resolved.display()))
    }

    /// 取出 Tasker 的克隆，供后台线程执行任务（Tasker 实现了 Clone）
    pub fn tasker_clone(&self) -> Result<Tasker, String> {
        let inner = self.lock()?;
        inner.tasker.clone().ok_or_else(|| "尚未加载资源".to_string())
    }

    /// 取出控制器的克隆，供运行时的事件回调在识别命中时抓帧（Controller 实现了 Clone）
    pub fn controller_clone(&self) -> Result<Controller, String> {
        let inner = self.lock()?;
        inner
            .controller
            .clone()
            .ok_or_else(|| "尚未连接控制器".to_string())
    }

    /// 主动向控制器截一帧并保存到文件，便于前端展示「当前屏幕」。
    /// 若尚未连接控制器则返回错误提示。
    pub fn controller_screenshot(&self, output: &str) -> Result<String, String> {
        let controller = self.lock()?.controller.clone().ok_or("尚未连接控制器")?;
        // 先触发一次截屏，wait 完成后再取缓存帧，保证拿到的是最新画面
        let job = controller.post_screencap().map_err(|e| e.to_string())?;
        let _ = controller.wait(job);
        let image = controller.cached_image().map_err(|e| e.to_string())?;
        let bytes = image.to_vec().ok_or("无法编码控制器截图")?;
        let path = resolve_existing_path_allow_missing(output);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
        Ok(path.to_string_lossy().to_string())
    }

    /// 在阻塞线程中执行任务：post_task 返回的 Job 需 wait 才是同步等待完成
    pub fn run_task_blocking(tasker: Tasker, entry: &str) -> Result<String, String> {
        if !tasker.inited() {
            return Err("资源或控制器尚未绑定，请先加载资源并连接设备".to_string());
        }
        let job = tasker.post_task(entry, "{}").map_err(|e| e.to_string())?;
        let _ = job.wait();
        Ok(format!("任务 {} 执行完成", entry))
    }

    /// 请求停止当前任务
    pub fn stop(&self) -> Result<String, String> {
        let tasker = self.tasker_clone()?;
        tasker.post_stop().map_err(|e| e.to_string())?;
        Ok("已请求停止".to_string())
    }

    /// 汇总当前运行时状态，便于前端排查
    pub fn status(&self) -> Result<String, String> {
        let inner = self.lock()?;
        let mut parts = vec![format!("库已加载={}", inner.library_loaded)];

        if let Some(tasker) = &inner.tasker {
            parts.push(format!("Tasker已初始化={}", tasker.inited()));
            let resource_loaded = tasker.resource().map(|res| res.loaded()).unwrap_or(false);
            parts.push(format!("资源已加载={}", resource_loaded));
        } else {
            parts.push("Tasker未创建".to_string());
        }

        if let Some(controller) = &inner.controller {
            parts.push(format!("控制器已连接={}", controller.connected()));
            if controller.connected() {
                if let Ok((width, height)) = controller.resolution() {
                    parts.push(format!("分辨率={}x{}", width, height));
                }
            }
        } else {
            parts.push("控制器未连接".to_string());
        }

        Ok(parts.join(" | "))
    }

    /// 连接设备后统一处理：等待连接结果、按需绑定 Tasker、保存控制器
    fn attach_controller(&self, controller: Controller, label: String) -> Result<String, String> {
        let connection_id = controller.post_connection().map_err(|e| e.to_string())?;
        let _ = controller.wait(connection_id);

        if !controller.connected() {
            return Err(format!("{} 连接失败", label));
        }

        let mut inner = self.lock()?;
        // 若资源已先加载（Tasker 已存在），则立即把控制器补绑上去
        if let Some(tasker) = &inner.tasker {
            tasker.bind_controller(&controller).map_err(|e| e.to_string())?;
        }
        inner.controller = Some(controller);
        Ok(format!("{} 已连接", label))
    }

    fn ensure_loaded(&self) -> Result<(), String> {
        let inner = self.lock()?;
        if !inner.library_loaded {
            return Err("尚未加载 MaaFramework 动态库，请先点击「加载动态库」".to_string());
        }
        Ok(())
    }

    fn lock(&self) -> Result<MutexGuard<'_, RuntimeInner>, String> {
        self.inner
            .lock()
            .map_err(|e| format!("运行时状态锁损坏: {}", e))
    }
}

/// 解析用户输入的读取路径。
/// 程序可能从任意目录启动（`tauri dev` 时 cwd 是 src-tauri，直接跑 release exe 时是 target/release），
/// 而 SDK 与资源位于仓库根目录，因此沿当前目录与可执行文件目录**逐级向上**查找。
pub fn resolve_existing_path(input: &str) -> PathBuf {
    let direct = PathBuf::from(input);
    if direct.is_absolute() || direct.exists() {
        return direct;
    }
    resolve_in_bases(input, &candidate_bases(), false).unwrap_or(direct)
}

/// 解析写入目标路径：目标本身可能尚不存在，此时落到「父目录已存在」的候选，
/// 保证模板图与截图写进既有的 resource/ 目录，而不是散落到构建产物里。
pub fn resolve_existing_path_allow_missing(input: &str) -> PathBuf {
    let direct = PathBuf::from(input);
    if direct.is_absolute() || direct.exists() {
        return direct;
    }
    let bases = candidate_bases();
    resolve_in_bases(input, &bases, true)
        .or_else(|| bases.first().map(|base| base.join(&direct)))
        .unwrap_or(direct)
}

/// 在候选基目录中解析相对路径。
/// `require_parent` 为真时匹配「父目录已存在」的候选（写入场景），否则匹配「自身存在」的候选（读取场景）。
fn resolve_in_bases(input: &str, bases: &[PathBuf], require_parent: bool) -> Option<PathBuf> {
    let direct = PathBuf::from(input);
    for base in bases {
        let candidate = base.join(&direct);
        if require_parent {
            if candidate.parent().map(|dir| dir.exists()).unwrap_or(false) {
                return Some(candidate);
            }
        } else if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

/// 可执行文件所在目录；取不到时退回当前目录
fn current_exe_dir() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            return dir.to_path_buf();
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// 候选基目录：当前目录及其各级上级，随后是可执行文件所在目录及其各级上级。
fn candidate_bases() -> Vec<PathBuf> {
    let mut bases = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        push_ancestors(&mut bases, &cwd);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            push_ancestors(&mut bases, dir);
        }
    }
    bases
}

/// 把目录自身与其各级上级目录加入候选，最多向上 8 层
fn push_ancestors(bases: &mut Vec<PathBuf>, start: &Path) {
    let mut current = start.to_path_buf();
    for _ in 0..8 {
        if current.as_os_str().is_empty() {
            break;
        }
        bases.push(current.clone());
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("maawiz_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn ancestors_are_collected_upward() {
        let mut bases = Vec::new();
        push_ancestors(&mut bases, Path::new("a/b/c"));
        assert_eq!(
            bases,
            vec![PathBuf::from("a/b/c"), PathBuf::from("a/b"), PathBuf::from("a")]
        );
    }

    #[test]
    fn reads_existing_file_from_a_deeper_base() {
        let dir = temp_dir("read");
        let nested = dir.join("a/b/c");
        std::fs::create_dir_all(nested.join("maa-sdk/bin")).unwrap();
        std::fs::write(nested.join("maa-sdk/bin/MaaFramework.dll"), b"x").unwrap();

        // 前两个候选下没有该文件，应继续向下找
        let bases = vec![dir.join("x/y"), dir.join("a/b"), nested.clone()];
        let resolved = resolve_in_bases("maa-sdk/bin/MaaFramework.dll", &bases, false).unwrap();
        assert_eq!(resolved, nested.join("maa-sdk/bin/MaaFramework.dll"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn write_target_lands_where_parent_directory_exists() {
        let dir = temp_dir("write");
        std::fs::create_dir_all(dir.join("resource")).unwrap();

        // 第一个候选（模拟 target/release）下没有 resource，应跳过它
        let bases = vec![dir.join("target/release"), dir.clone()];
        let resolved = resolve_in_bases("resource/.screenshot.png", &bases, true).unwrap();
        assert_eq!(resolved, dir.join("resource/.screenshot.png"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn resolve_returns_none_when_no_base_matches() {
        let bases = vec![PathBuf::from("definitely/not/here")];
        assert!(resolve_in_bases("nope/file.txt", &bases, false).is_none());
        assert!(resolve_in_bases("nope/file.txt", &bases, true).is_none());
    }

    /// 回归测试：从构建产物所在的多层嵌套目录出发，也能定位到仓库根的 SDK 与资源。
    /// 依赖 `make fetch-sdk` 已执行，缺失时跳过而非误报失败。
    #[test]
    fn sdk_and_resource_are_reachable_from_nested_directory() {
        if !resolve_existing_path("maa-sdk/bin/MaaFramework.dll").exists() {
            eprintln!("跳过：未找到 maa-sdk，请先执行 make fetch-sdk");
            return;
        }
        assert!(resolve_existing_path("maa-sdk/bin/MaaFramework.dll").is_file());
        assert!(resolve_existing_path("resource").is_dir());
    }

    #[test]
    fn absolute_paths_are_kept_as_is() {
        let absolute = if cfg!(windows) { "C:/definitely/not/here.txt" } else { "/definitely/not/here.txt" };
        assert_eq!(resolve_existing_path(absolute), PathBuf::from(absolute));
        assert_eq!(
            resolve_existing_path_allow_missing(absolute),
            PathBuf::from(absolute)
        );
    }
}
