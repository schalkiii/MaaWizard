use std::collections::HashMap;

use serde_json::{Map, Value};

/// Pipeline 协议版本。V1 为扁平字段，V2 将识别/动作参数收拢到 {type, param}。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineVersion {
    V1,
    V2,
}

impl PipelineVersion {
    pub fn from_str_checked(text: &str) -> Result<Self, String> {
        match text.trim().to_ascii_uppercase().as_str() {
            "V1" | "1" => Ok(PipelineVersion::V1),
            "V2" | "2" => Ok(PipelineVersion::V2),
            other => Err(format!("未知的 Pipeline 版本: {}", other)),
        }
    }
}

/// V1 扁平协议下，用于判定字段归属的字段名集合。
/// 未收录的字段会进入 `extra`，保证导入导出不丢数据（round-trip 保真）。
const RECOGNITION_FIELDS: &[&str] = &[
    "roi",
    "roi_offset",
    "template",
    "threshold",
    "method",
    "green_mask",
    "detector",
    "ratio",
    "count",
    "lower",
    "upper",
    "connected",
    "expected",
    "model",
    "labels",
    "color_filter",
    "all_of",
    "any_of",
    "custom_recognition",
    "custom_recognition_param",
    "order_by",
    "index",
    "only_recognized",
];

const ACTION_FIELDS: &[&str] = &[
    "target",
    "target_offset",
    "contact",
    "pressure",
    "begin",
    "end",
    "duration",
    "dx",
    "dy",
    "input_text",
    "package",
    "exec",
    "args",
    "detach",
    "cmd",
    "shell_timeout",
    "filename",
    "format",
    "custom_action",
    "custom_action_param",
];

const NODE_FIELDS: &[&str] = &[
    "recognition",
    "action",
    "next",
    "on_error",
    "interrupt",
    "is_sub",
    "timeout",
    "rate_limit",
    "anchor",
    "inverse",
    "enabled",
    "max_hit",
    "pre_delay",
    "post_delay",
    "pre_wait_freezes",
    "post_wait_freezes",
    "repeat",
    "repeat_delay",
    "repeat_wait_freezes",
    "focus",
    "attach",
];

/// 识别规格：类型名 + 参数表
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RecognitionSpec {
    pub kind: String,
    pub param: Map<String, Value>,
}

/// 动作规格：类型名 + 参数表
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ActionSpec {
    pub kind: String,
    pub param: Map<String, Value>,
}

/// next / on_error 的条目：节点名（可为 "[JumpBack]" 特殊标记）或 v5.1+ 的对象形式
#[derive(Debug, Clone, PartialEq)]
pub enum NextEntry {
    Name(String),
    Object(Map<String, Value>),
}

impl NextEntry {
    /// 取出条目指向的节点名，对象形式取 name 字段
    pub fn node_name(&self) -> Option<&str> {
        match self {
            NextEntry::Name(name) => Some(name.as_str()),
            NextEntry::Object(map) => map.get("name").and_then(|v| v.as_str()),
        }
    }

    pub fn to_json(&self) -> Value {
        match self {
            NextEntry::Name(name) => Value::String(name.clone()),
            NextEntry::Object(map) => Value::Object(map.clone()),
        }
    }

    pub fn from_json(value: &Value) -> Result<Self, String> {
        match value {
            Value::String(name) => Ok(NextEntry::Name(name.clone())),
            Value::Object(map) => Ok(NextEntry::Object(map.clone())),
            other => Err(format!("next/on_error 条目必须是字符串或对象，实际为 {}", other)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PipelineNode {
    pub recognition: Option<RecognitionSpec>,
    pub action: Option<ActionSpec>,
    pub next: Vec<NextEntry>,
    pub on_error: Vec<NextEntry>,
    pub timeout: Option<i64>,
    pub rate_limit: Option<u64>,
    pub inverse: Option<bool>,
    pub enabled: Option<bool>,
    pub max_hit: Option<u32>,
    pub pre_delay: Option<u64>,
    pub post_delay: Option<u64>,
    pub repeat: Option<u32>,
    pub focus: Option<bool>,
    /// 未收录的字段原样保留，避免导入导出丢失信息
    pub extra: Map<String, Value>,
}

impl PipelineNode {
    /// 从节点 JSON 解析，自动识别 V1（扁平）与 V2（{type, param}）
    pub fn from_json(node: &Map<String, Value>) -> Result<Self, String> {
        let mut result = PipelineNode::default();

        // 解析识别：V2 为对象 {"type": "OCR", "param": {...}}，V1 为字符串
        if let Some(recognition) = node.get("recognition") {
            let (kind, param) = parse_spec(recognition, RECOGNITION_FIELDS, node, "recognition")?;
            result.recognition = Some(RecognitionSpec { kind, param });
        }

        // 解析动作，规则同上
        if let Some(action) = node.get("action") {
            let (kind, param) = parse_spec(action, ACTION_FIELDS, node, "action")?;
            result.action = Some(ActionSpec { kind, param });
        }

        if let Some(next) = node.get("next") {
            result.next = parse_entries(next)?;
        }
        if let Some(on_error) = node.get("on_error") {
            result.on_error = parse_entries(on_error)?;
        }

        result.timeout = node.get("timeout").and_then(|v| v.as_i64());
        result.rate_limit = node.get("rate_limit").and_then(|v| v.as_u64());
        result.inverse = node.get("inverse").and_then(|v| v.as_bool());
        result.enabled = node.get("enabled").and_then(|v| v.as_bool());
        result.max_hit = node
            .get("max_hit")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);
        result.pre_delay = node.get("pre_delay").and_then(|v| v.as_u64());
        result.post_delay = node.get("post_delay").and_then(|v| v.as_u64());
        result.repeat = node.get("repeat").and_then(|v| v.as_u64()).map(|v| v as u32);
        result.focus = node.get("focus").and_then(|v| v.as_bool());

        // 未识别的字段保留到 extra，保证 round-trip 不丢数据
        for (key, value) in node {
            if NODE_FIELDS.contains(&key.as_str())
                || RECOGNITION_FIELDS.contains(&key.as_str())
                || ACTION_FIELDS.contains(&key.as_str())
            {
                continue;
            }
            result.extra.insert(key.clone(), value.clone());
        }

        Ok(result)
    }

    /// 序列化为指定协议版本的节点 JSON
    pub fn to_json(&self, version: PipelineVersion) -> Map<String, Value> {
        let mut node = Map::new();

        if let Some(recognition) = &self.recognition {
            node.insert(
                "recognition".to_string(),
                spec_to_json(recognition.kind.as_str(), &recognition.param, version),
            );
            // V1 需要把识别参数平铺到节点上
            if version == PipelineVersion::V1 {
                for (key, value) in &recognition.param {
                    node.insert(key.clone(), value.clone());
                }
            }
        }

        if let Some(action) = &self.action {
            node.insert(
                "action".to_string(),
                spec_to_json(action.kind.as_str(), &action.param, version),
            );
            if version == PipelineVersion::V1 {
                for (key, value) in &action.param {
                    node.insert(key.clone(), value.clone());
                }
            }
        }

        if !self.next.is_empty() {
            node.insert(
                "next".to_string(),
                Value::Array(self.next.iter().map(|e| e.to_json()).collect()),
            );
        }
        if !self.on_error.is_empty() {
            node.insert(
                "on_error".to_string(),
                Value::Array(self.on_error.iter().map(|e| e.to_json()).collect()),
            );
        }

        // 仅在显式设置时输出，保持 JSON 简洁
        if let Some(value) = self.timeout {
            node.insert("timeout".to_string(), Value::from(value));
        }
        if let Some(value) = self.rate_limit {
            node.insert("rate_limit".to_string(), Value::from(value));
        }
        if let Some(value) = self.inverse {
            node.insert("inverse".to_string(), Value::from(value));
        }
        if let Some(value) = self.enabled {
            node.insert("enabled".to_string(), Value::from(value));
        }
        if let Some(value) = self.max_hit {
            node.insert("max_hit".to_string(), Value::from(value));
        }
        if let Some(value) = self.pre_delay {
            node.insert("pre_delay".to_string(), Value::from(value));
        }
        if let Some(value) = self.post_delay {
            node.insert("post_delay".to_string(), Value::from(value));
        }
        if let Some(value) = self.repeat {
            node.insert("repeat".to_string(), Value::from(value));
        }
        if let Some(value) = self.focus {
            node.insert("focus".to_string(), Value::from(value));
        }

        for (key, value) in &self.extra {
            node.insert(key.clone(), value.clone());
        }

        node
    }

    /// 生成新节点时的默认识别/动作
    pub fn with_defaults() -> Self {
        Self {
            recognition: Some(RecognitionSpec {
                kind: "DirectHit".to_string(),
                param: Map::new(),
            }),
            action: Some(ActionSpec {
                kind: "DoNothing".to_string(),
                param: Map::new(),
            }),
            ..PipelineNode::default()
        }
    }
}

/// 解析 V1/V2 的识别或动作定义，返回 (类型名, 参数表)。
/// 识别与动作共用同一套解析规则，因此这里不绑定具体结构体。
fn parse_spec(
    value: &Value,
    known_fields: &[&str],
    node: &Map<String, Value>,
    label: &str,
) -> Result<(String, Map<String, Value>), String> {
    match value {
        // V2：{"type": "OCR", "param": {...}}
        Value::Object(spec) => {
            let kind = spec
                .get("type")
                .and_then(|v| v.as_str())
                .ok_or_else(|| format!("{} 的对象形式缺少 type 字段", label))?;
            let param = match spec.get("param") {
                Some(Value::Object(map)) => map.clone(),
                Some(other) => {
                    return Err(format!("{} 的 param 必须是对象，实际为 {}", label, other))
                }
                None => Map::new(),
            };
            Ok((kind.to_string(), param))
        }
        // V1：字段名即类型，其余参数从节点上按字段名集合收集
        Value::String(kind) => {
            let mut param = Map::new();
            for field in known_fields {
                if let Some(value) = node.get(*field) {
                    param.insert(field.to_string(), value.clone());
                }
            }
            Ok((kind.clone(), param))
        }
        other => Err(format!(
            "{} 必须是字符串(V1)或对象(V2)，实际为 {}",
            label, other
        )),
    }
}

fn spec_to_json(kind: &str, param: &Map<String, Value>, version: PipelineVersion) -> Value {
    match version {
        PipelineVersion::V1 => Value::String(kind.to_string()),
        PipelineVersion::V2 => {
            let mut spec = Map::new();
            spec.insert("type".to_string(), Value::String(kind.to_string()));
            spec.insert(
                "param".to_string(),
                Value::Object(param.clone()),
            );
            Value::Object(spec)
        }
    }
}

fn parse_entries(value: &Value) -> Result<Vec<NextEntry>, String> {
    match value {
        Value::Array(items) => items.iter().map(NextEntry::from_json).collect(),
        // 部分旧项目会把单个后继写成字符串
        other => Ok(vec![NextEntry::from_json(other)?]),
    }
}

/// 一份流水线文档，即 Pipeline JSON 的内存表示（ADR 0002 中的唯一真相源）
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PipelineDocument {
    pub nodes: HashMap<String, PipelineNode>,
}

impl PipelineDocument {
    pub fn from_json(value: &Value) -> Result<Self, String> {
        let map = value
            .as_object()
            .ok_or_else(|| "Pipeline 根节点必须是 JSON 对象".to_string())?;

        let mut nodes = HashMap::new();
        for (name, node_value) in map {
            // 以 $ 开头的键是 MaaFramework 的保留字段，不解析为节点
            if name.starts_with('$') {
                continue;
            }
            let node_map = node_value
                .as_object()
                .ok_or_else(|| format!("节点 {} 必须是 JSON 对象", name))?;
            nodes.insert(name.clone(), PipelineNode::from_json(node_map)?);
        }

        Ok(PipelineDocument { nodes })
    }

    pub fn to_json(&self, version: PipelineVersion) -> Value {
        let mut map = Map::new();
        for (name, node) in &self.nodes {
            map.insert(name.clone(), Value::Object(node.to_json(version)));
        }
        Value::Object(map)
    }

    /// 生成一个当前文档中不存在的新节点名
    pub fn unique_name(&self, prefix: &str) -> String {
        if !self.nodes.contains_key(prefix) {
            return prefix.to_string();
        }
        for index in 1..10000 {
            let candidate = format!("{}{}", prefix, index);
            if !self.nodes.contains_key(&candidate) {
                return candidate;
            }
        }
        format!("{}{}", prefix, chrono::Utc::now().timestamp_millis())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 一份覆盖了识别参数、动作、next/on_error、节点字段的 V1 节点
    fn v1_node() -> Value {
        json!({
            "recognition": "TemplateMatch",
            "template": "btn_start.png",
            "threshold": 0.8,
            "roi": [10, 20, 100, 50],
            "action": "Click",
            "next": ["Step2", "[JumpBack]"],
            "on_error": "Retry",
            "timeout": 5000,
            "inverse": true,
            "pre_delay": 200,
            "focus": true
        })
    }

    #[test]
    fn v1_node_parses_all_fields() {
        let node = PipelineNode::from_json(v1_node().as_object().unwrap()).unwrap();

        let recognition = node.recognition.unwrap();
        assert_eq!(recognition.kind, "TemplateMatch");
        assert_eq!(recognition.param.get("template").unwrap(), "btn_start.png");
        assert_eq!(recognition.param.get("threshold").unwrap().as_f64().unwrap(), 0.8);
        assert_eq!(recognition.param.get("roi").unwrap(), &json!([10, 20, 100, 50]));

        assert_eq!(node.action.unwrap().kind, "Click");

        assert_eq!(node.next.len(), 2);
        assert_eq!(node.next[0].node_name(), Some("Step2"));
        assert_eq!(node.next[1].node_name(), Some("[JumpBack]"));
        assert_eq!(node.on_error[0].node_name(), Some("Retry"));

        assert_eq!(node.timeout, Some(5000));
        assert_eq!(node.inverse, Some(true));
        assert_eq!(node.pre_delay, Some(200));
        assert_eq!(node.focus, Some(true));
        assert!(node.extra.is_empty());
    }

    #[test]
    fn v1_round_trip_is_stable() {
        let node = PipelineNode::from_json(v1_node().as_object().unwrap()).unwrap();
        let round = node.to_json(PipelineVersion::V1);
        let round_node = PipelineNode::from_json(&round).unwrap();
        assert_eq!(node, round_node);
    }

    #[test]
    fn v2_round_trip_is_stable() {
        let node = PipelineNode::from_json(v1_node().as_object().unwrap()).unwrap();
        let v2 = node.to_json(PipelineVersion::V2);

        // V2 的识别收拢为 {type, param}，参数不再平铺
        let recognition = v2.get("recognition").unwrap();
        assert_eq!(recognition["type"], "TemplateMatch");
        assert_eq!(recognition["param"]["template"], "btn_start.png");
        assert!(v2.get("template").is_none());

        let parsed_back = PipelineNode::from_json(&v2).unwrap();
        assert_eq!(node, parsed_back);
    }

    #[test]
    fn unknown_fields_are_preserved_in_extra() {
        let value = json!({
            "recognition": "OCR",
            "expected": "开始",
            "action": "Click",
            "some_future_field": {"a": 1}
        });
        let node = PipelineNode::from_json(value.as_object().unwrap()).unwrap();
        assert_eq!(node.extra.get("some_future_field").unwrap(), &json!({"a": 1}));
    }

    #[test]
    fn v2_object_without_type_is_rejected() {
        let value = json!({"recognition": {"param": {}}, "action": "Click"});
        let error = PipelineNode::from_json(value.as_object().unwrap()).unwrap_err();
        assert!(error.contains("type"), "错误应定位到 type 字段，实际为 {}", error);
    }

    #[test]
    fn next_object_form_is_preserved() {
        let value = json!({
            "recognition": "DirectHit",
            "action": "DoNothing",
            "next": [{"name": "Next", "type": "stop"}]
        });
        let node = PipelineNode::from_json(value.as_object().unwrap()).unwrap();
        assert_eq!(node.next[0].node_name(), Some("Next"));
        assert_eq!(node.next[0].to_json(), json!({"name": "Next", "type": "stop"}));
    }

    #[test]
    fn document_round_trip_skips_reserved_keys() {
        let document = json!({
            "Start": v1_node(),
            "$schema": "这里是不应被当作节点的保留字段",
            "Next": {"recognition": "DirectHit", "action": "DoNothing"}
        });
        let parsed = PipelineDocument::from_json(&document).unwrap();
        assert_eq!(parsed.nodes.len(), 2);
        assert!(!parsed.nodes.contains_key("$schema"));

        let out = parsed.to_json(PipelineVersion::V1);
        let parsed_again = PipelineDocument::from_json(&out).unwrap();
        assert_eq!(parsed, parsed_again);
    }

    #[test]
    fn document_unique_name_avoids_collision() {
        let mut document = PipelineDocument::default();
        document.nodes.insert("Step".to_string(), PipelineNode::default());
        assert_eq!(document.unique_name("Step"), "Step1");
        assert_eq!(document.unique_name("New"), "New");
    }

    #[test]
    fn version_parsing_accepts_aliases() {
        assert_eq!(PipelineVersion::from_str_checked("v1").unwrap(), PipelineVersion::V1);
        assert_eq!(PipelineVersion::from_str_checked("2").unwrap(), PipelineVersion::V2);
        assert!(PipelineVersion::from_str_checked("v3").is_err());
    }
}
