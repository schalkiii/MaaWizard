use std::os::raw::c_void;
use std::path::PathBuf;
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
        maa_framework::load_library(&resolved).map_err(|e| e.to_string())?;

        let user_path = std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join("maa_userdata");
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

/// 解析用户输入的路径。
/// `tauri dev` 下当前目录是 src-tauri，而 SDK 与资源位于仓库根目录，
/// 因此依次尝试：原样 → 当前目录 → 上级目录 → 可执行文件所在目录。
pub fn resolve_existing_path(input: &str) -> PathBuf {
    let direct = PathBuf::from(input);
    if direct.exists() {
        return direct;
    }

    if let Ok(cwd) = std::env::current_dir() {
        let in_cwd = cwd.join(input);
        if in_cwd.exists() {
            return in_cwd;
        }
        if let Some(parent) = cwd.parent() {
            let in_parent = parent.join(input);
            if in_parent.exists() {
                return in_parent;
            }
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            let near_exe = exe_dir.join(input);
            if near_exe.exists() {
                return near_exe;
            }
        }
    }

    direct
}

/// 解析保存目标路径：目标本身可能尚不存在，此时基于仓库根目录推导，
/// 使 dev 模式下默认保存位置与资源目录保持一致。
pub fn resolve_existing_path_allow_missing(input: &str) -> PathBuf {
    let direct = PathBuf::from(input);
    if direct.is_absolute() || direct.exists() {
        return direct;
    }

    if let Ok(cwd) = std::env::current_dir() {
        if let Some(parent) = cwd.parent() {
            let candidate = parent.join(&direct);
            if candidate
                .parent()
                .map(|dir| dir.exists())
                .unwrap_or(false)
            {
                return candidate;
            }
        }
        return cwd.join(&direct);
    }

    direct
}
