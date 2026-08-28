use std::process::Command;

use serde::Serialize;

/// AI 能力所需的外部运行环境探测结果（见 ADR 0004：不内置 Python，优雅降级）
#[derive(Debug, Clone, Serialize)]
pub struct AiEnvironment {
    pub python: Option<String>,
    pub uv: Option<String>,
    pub uvx: Option<String>,
    /// 是否具备运行 AI 工具的最低条件，前端据此决定是否禁用 AI 面板
    pub usable: bool,
    pub suggestion: String,
}

/// 探测本机是否具备运行 maafw-cli / MaaMCP 的 Python 环境
pub fn detect_environment() -> AiEnvironment {
    let python = probe_version("python", &["--version"]);
    let uv = probe_version("uv", &["--version"]);
    let uvx = probe_version("uvx", &["--version"]);

    let suggestion = if uvx.is_some() {
        "已检测到 uvx，可直接运行 maafw-cli / maa-mcp".to_string()
    } else if python.is_some() {
        "已检测到 python，可通过 pip 安装 maafw-cli 与 maa-mcp".to_string()
    } else {
        "未检测到 Python 环境，AI 功能暂不可用。请安装 Python 3.10+ 或 uv".to_string()
    };

    let usable = uvx.is_some() || python.is_some();

    AiEnvironment {
        python,
        uv,
        uvx,
        usable,
        suggestion,
    }
}

/// 运行一条 AI 工具命令（如 maafw-cli device / ocr），返回合并后的输出
pub fn run_tool(program: String, args: Vec<String>) -> Result<String, String> {
    let output = Command::new(&program)
        .args(&args)
        .output()
        .map_err(|e| format!("启动 {} 失败：{}", program, e))?;

    let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !stderr.trim().is_empty() {
        combined.push_str("\n[stderr]\n");
        combined.push_str(&stderr);
    }

    if output.status.success() {
        Ok(combined)
    } else {
        Err(format!("命令退出码 {:?}\n{}", output.status.code(), combined))
    }
}

fn probe_version(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    let fallback = String::from_utf8_lossy(&output.stderr).to_string();
    let raw = if text.trim().is_empty() { fallback } else { text };
    Some(raw.trim().to_string())
}
