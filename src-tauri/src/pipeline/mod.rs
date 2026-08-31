pub mod model;
pub mod validate;

pub use model::*;
pub use validate::ValidationIssue;

use validate::{has_errors, IssueLevel};

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde_json::Value;

/// 内存中的 PipelineDocument 及其来源路径。
/// 按 ADR 0002，文档的唯一真相源在 Rust 侧：录制引擎往这里追加节点，
/// 图编辑器从这里取快照渲染，所有编辑都通过命令提交回这里。
pub struct PipelineState {
    document: Mutex<PipelineDocument>,
    source_path: Mutex<Option<PathBuf>>,
}

impl Default for PipelineState {
    fn default() -> Self {
        Self {
            document: Mutex::new(PipelineDocument::default()),
            source_path: Mutex::new(None),
        }
    }
}

impl PipelineState {
    /// 打开资源包：递归读取 <bundle>/pipeline/ 下所有 json；若传入的是文件则直接读取
    pub fn open(&self, path: &str) -> Result<String, String> {
        let resolved = crate::maa::resolve_existing_path(path);
        let mut merged = PipelineDocument::default();
        let mut loaded_files = 0usize;

        if resolved.is_file() {
            merged = load_file(&resolved)?;
            loaded_files = 1;
        } else if resolved.is_dir() {
            let pipeline_dir = resolved.join("pipeline");
            let target_dir = if pipeline_dir.is_dir() {
                pipeline_dir
            } else {
                resolved.clone()
            };
            for entry in collect_json_files(&target_dir)? {
                let document = load_file(&entry)?;
                merged.nodes.extend(document.nodes);
                loaded_files += 1;
            }
        } else {
            return Err(format!("路径不存在：{}", resolved.display()));
        }

        let node_count = merged.nodes.len();
        *self.document.lock().map_err(lock_error)? = merged;
        *self.source_path.lock().map_err(lock_error)? = Some(resolved);

        Ok(format!(
            "已加载 {} 个文件、{} 个节点",
            loaded_files, node_count
        ))
    }

    /// 保存到资源包的 pipeline 目录，可指定导出版本
    pub fn save(&self, path: Option<String>, version: PipelineVersion) -> Result<String, String> {
        let target_dir = match path {
            Some(given) => crate::maa::resolve_existing_path_allow_missing(&given),
            None => self
                .source_path
                .lock()
                .map_err(lock_error)?
                .clone()
                .ok_or_else(|| "尚未打开资源包，请指定保存路径".to_string())?,
        };

        let pipeline_dir = if target_dir.file_name().and_then(|n| n.to_str()) == Some("pipeline") {
            target_dir
        } else {
            target_dir.join("pipeline")
        };
        std::fs::create_dir_all(&pipeline_dir).map_err(|e| e.to_string())?;

        let file_path = pipeline_dir.join("maawizard.json");
        let document = self.document.lock().map_err(lock_error)?;
        let json = serde_json::to_string_pretty(&document.to_json(version))
            .map_err(|e| e.to_string())?;
        std::fs::write(&file_path, json).map_err(|e| e.to_string())?;

        // 顺带给一次校验提示，避免把错误一直留到运行时才暴露
        let mut message = format!(
            "已保存 {} 个节点到 {}（协议 {:?}）",
            document.nodes.len(),
            file_path.display(),
            version
        );
        let issues = validate::validate(&document);
        if has_errors(&issues) {
            let error_count = issues
                .iter()
                .filter(|item| item.level == IssueLevel::Error)
                .count();
            message.push_str(&format!("；注意：存在 {} 处校验错误，建议先「校验」修复", error_count));
        }

        Ok(message)
    }

    /// 校验当前文档，返回可定位到字段的问题列表
    pub fn validate(&self) -> Result<Vec<ValidationIssue>, String> {
        let document = self.document.lock().map_err(lock_error)?;
        Ok(crate::pipeline::validate::validate(&document))
    }

    pub fn snapshot(&self) -> Result<Value, String> {
        let document = self.document.lock().map_err(lock_error)?;
        Ok(document.to_json(PipelineVersion::V2))
    }

    pub fn update_node(&self, name: &str, node_json: Value) -> Result<String, String> {
        let node_map = node_json
            .as_object()
            .ok_or_else(|| "节点数据必须是 JSON 对象".to_string())?;
        let node = PipelineNode::from_json(node_map)?;

        let mut document = self.document.lock().map_err(lock_error)?;
        document.nodes.insert(name.to_string(), node);
        Ok(format!("节点 {} 已更新", name))
    }

    pub fn add_node(&self, name: Option<String>) -> Result<String, String> {
        let mut document = self.document.lock().map_err(lock_error)?;
        let node_name = match name {
            Some(given) if !given.trim().is_empty() => given,
            _ => document.unique_name("Node"),
        };
        if document.nodes.contains_key(&node_name) {
            return Err(format!("节点 {} 已存在", node_name));
        }
        document
            .nodes
            .insert(node_name.clone(), PipelineNode::with_defaults());
        Ok(node_name)
    }

    pub fn delete_node(&self, name: &str) -> Result<String, String> {
        let mut document = self.document.lock().map_err(lock_error)?;
        if document.nodes.remove(name).is_none() {
            return Err(format!("节点 {} 不存在", name));
        }
        // 同时清理其它节点中指向它的跳转，避免出现悬空引用
        for node in document.nodes.values_mut() {
            node.next.retain(|entry| entry.node_name() != Some(name));
            node.on_error.retain(|entry| entry.node_name() != Some(name));
        }
        Ok(format!("节点 {} 已删除，相关跳转已清理", name))
    }

    pub fn document_guard(&self) -> Result<MutexGuard<'_, PipelineDocument>, String> {
        self.document.lock().map_err(lock_error)
    }
}

fn load_file(path: &Path) -> Result<PipelineDocument, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("读取 {} 失败：{}", path.display(), e))?;
    let value: Value = serde_json::from_str(&content)
        .map_err(|e| format!("解析 {} 失败：{}", path.display(), e))?;
    PipelineDocument::from_json(&value)
}

/// 递归收集目录下所有 json 文件；以 . 开头的目录按 MaaFramework 约定忽略
fn collect_json_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];

    while let Some(current) = stack.pop() {
        let entries = std::fs::read_dir(&current)
            .map_err(|e| format!("读取目录 {} 失败：{}", current.display(), e))?;
        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if !file_name.starts_with('.') {
                    stack.push(path);
                }
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                files.push(path);
            }
        }
    }

    Ok(files)
}

fn lock_error<T>(_value: T) -> String {
    "Pipeline 状态锁损坏".to_string()
}
