//! Pipeline 文档校验（对应 test-plan.md §1「Schema 校验」）。
//!
//! 在保存/运行前拦截非法节点，把问题定位到具体字段，
//! 避免把错误留到 MaaFramework 运行时才以难以理解的形式暴露。

use serde::Serialize;

use super::model::{PipelineDocument, PipelineNode};

/// 问题级别：error 会阻止运行，warning 仅提示风险
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum IssueLevel {
    Error,
    Warning,
}

/// 一条校验问题，可定位到「节点 + 字段」
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ValidationIssue {
    /// 节点名；文档级别的问题为空字符串
    pub node: String,
    pub level: IssueLevel,
    /// 字段路径，如 `recognition.template`、`next`
    pub field: String,
    pub message: String,
}

/// MaaFramework 支持的识别类型（与前端 help/registry.ts 保持一致）
pub const RECOGNITION_KINDS: &[&str] = &[
    "DirectHit",
    "TemplateMatch",
    "FeatureMatch",
    "ColorMatch",
    "OCR",
    "NeuralNetworkClassify",
    "NeuralNetworkDetect",
    "And",
    "Or",
    "Custom",
];

/// MaaFramework 支持的动作类型
pub const ACTION_KINDS: &[&str] = &[
    "DoNothing",
    "Click",
    "LongPress",
    "Swipe",
    "MultiSwipe",
    "InputText",
    "ClickKey",
    "StartApp",
    "StopApp",
    "StopTask",
    "Scroll",
    "Command",
    "Shell",
    "Screencap",
    "Custom",
];

/// 各识别类型的必填参数
fn required_recognition_params(kind: &str) -> &'static [&'static str] {
    match kind {
        "TemplateMatch" | "FeatureMatch" => &["template"],
        "OCR" | "NeuralNetworkClassify" | "NeuralNetworkDetect" => &["expected"],
        "ColorMatch" => &["lower", "upper"],
        "And" => &["all_of"],
        "Or" => &["any_of"],
        "Custom" => &["custom_recognition"],
        _ => &[],
    }
}

/// 各动作类型的必填参数
fn required_action_params(kind: &str) -> &'static [&'static str] {
    match kind {
        "InputText" => &["input_text"],
        "ClickKey" => &["key"],
        "Swipe" | "MultiSwipe" => &["begin", "end"],
        "StartApp" | "StopApp" => &["package"],
        "Shell" => &["cmd"],
        "Command" => &["exec"],
        "Custom" => &["custom_action"],
        _ => &[],
    }
}

fn issue(node: &str, level: IssueLevel, field: impl Into<String>, message: impl Into<String>) -> ValidationIssue {
    ValidationIssue {
        node: node.to_string(),
        level,
        field: field.into(),
        message: message.into(),
    }
}

/// 校验整份文档，返回按节点名排序的问题列表
pub fn validate(document: &PipelineDocument) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    if document.nodes.is_empty() {
        issues.push(issue(
            "",
            IssueLevel::Warning,
            "",
            "文档为空，没有可运行的节点",
        ));
        return issues;
    }

    let mut names: Vec<&String> = document.nodes.keys().collect();
    names.sort();

    for name in names {
        let node = &document.nodes[name.as_str()];
        validate_recognition(name, node, &mut issues);
        validate_action(name, node, &mut issues);
        validate_targets(name, node, document, &mut issues);
        validate_fragility(name, node, &mut issues);
    }

    issues
}

/// 只含 error 级别问题，供运行前快速判定
pub fn has_errors(issues: &[ValidationIssue]) -> bool {
    issues.iter().any(|item| item.level == IssueLevel::Error)
}

fn validate_recognition(name: &str, node: &PipelineNode, issues: &mut Vec<ValidationIssue>) {
    let Some(recognition) = &node.recognition else {
        issues.push(issue(
            name,
            IssueLevel::Warning,
            "recognition",
            "未指定识别方式，运行时按 DirectHit 处理",
        ));
        return;
    };

    if !RECOGNITION_KINDS.contains(&recognition.kind.as_str()) {
        issues.push(issue(
            name,
            IssueLevel::Error,
            "recognition",
            format!("未知的识别类型：{}", recognition.kind),
        ));
        // 类型都错了，其参数无从判断，不再继续报
        return;
    }

    for field in required_recognition_params(&recognition.kind) {
        if recognition.param.get(*field).is_none() {
            issues.push(issue(
                name,
                IssueLevel::Error,
                format!("recognition.{}", field),
                format!("{} 缺少必填参数 {}", recognition.kind, field),
            ));
        }
    }
}

fn validate_action(name: &str, node: &PipelineNode, issues: &mut Vec<ValidationIssue>) {
    let Some(action) = &node.action else {
        issues.push(issue(
            name,
            IssueLevel::Warning,
            "action",
            "未指定动作，运行时按 DoNothing 处理",
        ));
        return;
    };

    if !ACTION_KINDS.contains(&action.kind.as_str()) {
        issues.push(issue(
            name,
            IssueLevel::Error,
            "action",
            format!("未知的动作类型：{}", action.kind),
        ));
        return;
    }

    for field in required_action_params(&action.kind) {
        if action.param.get(*field).is_none() {
            issues.push(issue(
                name,
                IssueLevel::Error,
                format!("action.{}", field),
                format!("{} 缺少必填参数 {}", action.kind, field),
            ));
        }
    }
}

/// 检查 next / on_error 是否指向了不存在的节点
fn validate_targets(
    name: &str,
    node: &PipelineNode,
    document: &PipelineDocument,
    issues: &mut Vec<ValidationIssue>,
) {
    for (field, entries) in [("next", &node.next), ("on_error", &node.on_error)] {
        for entry in entries {
            match entry.node_name() {
                // [JumpBack] 是框架约定的特殊标记，不指向具体节点
                Some("[JumpBack]") => {}
                Some(target) if document.nodes.contains_key(target) => {}
                Some(target) => issues.push(issue(
                    name,
                    IssueLevel::Error,
                    field,
                    format!("跳转到了不存在的节点：{}", target),
                )),
                None => issues.push(issue(
                    name,
                    IssueLevel::Error,
                    field,
                    "跳转条目缺少 name 字段",
                )),
            }
        }
    }
}

/// 提示坐标依赖带来的脆弱性（ADR 0003：抗变应靠重新识别，而非坐标）
fn validate_fragility(name: &str, node: &PipelineNode, issues: &mut Vec<ValidationIssue>) {
    let is_direct_hit = node
        .recognition
        .as_ref()
        .map(|recognition| recognition.kind == "DirectHit")
        .unwrap_or(true);

    let position_action = node
        .action
        .as_ref()
        .map(|action| matches!(action.kind.as_str(), "Click" | "LongPress" | "Swipe" | "MultiSwipe"))
        .unwrap_or(false);

    if !is_direct_hit || !position_action {
        return;
    }

    let has_roi = node
        .recognition
        .as_ref()
        .and_then(|recognition| recognition.param.get("roi"))
        .is_some();

    if !has_roi {
        issues.push(issue(
            name,
            IssueLevel::Warning,
            "recognition.roi",
            "未做识别且未限定 roi，属于纯坐标操作，换分辨率后即失效",
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::model::{ActionSpec, NextEntry, RecognitionSpec};
    use serde_json::json;

    fn recognition(kind: &str, param: serde_json::Value) -> Option<RecognitionSpec> {
        Some(RecognitionSpec {
            kind: kind.to_string(),
            param: param.as_object().unwrap().clone(),
        })
    }

    fn action(kind: &str, param: serde_json::Value) -> Option<ActionSpec> {
        Some(ActionSpec {
            kind: kind.to_string(),
            param: param.as_object().unwrap().clone(),
        })
    }

    fn document_with(nodes: Vec<(&str, PipelineNode)>) -> PipelineDocument {
        let mut document = PipelineDocument::default();
        for (name, node) in nodes {
            document.nodes.insert(name.to_string(), node);
        }
        document
    }

    #[test]
    fn valid_document_has_no_issues() {
        let document = document_with(vec![(
            "Start",
            PipelineNode {
                recognition: recognition("TemplateMatch", json!({ "template": "a.png" })),
                action: action("Click", json!({})),
                next: vec![NextEntry::Name("End".to_string())],
                ..PipelineNode::default()
            },
        ), (
            "End",
            PipelineNode {
                recognition: recognition("DirectHit", json!({ "roi": [0, 0, 10, 10] })),
                action: action("DoNothing", json!({})),
                ..PipelineNode::default()
            },
        )]);

        assert!(validate(&document).is_empty());
    }

    #[test]
    fn unknown_recognition_type_is_error() {
        let document = document_with(vec![(
            "Start",
            PipelineNode {
                recognition: recognition("TemplateMatching", json!({})),
                action: action("DoNothing", json!({})),
                ..PipelineNode::default()
            },
        )]);

        let issues = validate(&document);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].level, IssueLevel::Error);
        assert_eq!(issues[0].field, "recognition");
        // 类型未知时不应连带报缺参
        assert!(issues[0].message.contains("未知的识别类型"));
    }

    #[test]
    fn unknown_action_type_is_error() {
        let document = document_with(vec![(
            "Start",
            PipelineNode {
                recognition: recognition("DirectHit", json!({})),
                action: action("Clicky", json!({})),
                ..PipelineNode::default()
            },
        )]);

        let issues = validate(&document);
        assert!(issues.iter().any(|item| item.field == "action" && item.level == IssueLevel::Error));
    }

    #[test]
    fn missing_required_recognition_param_is_located_to_field() {
        let document = document_with(vec![(
            "Start",
            PipelineNode {
                recognition: recognition("TemplateMatch", json!({ "threshold": 0.8 })),
                action: action("DoNothing", json!({})),
                ..PipelineNode::default()
            },
        )]);

        let issues = validate(&document);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].field, "recognition.template");
        assert_eq!(issues[0].node, "Start");
    }

    #[test]
    fn color_match_requires_both_bounds() {
        let document = document_with(vec![(
            "Start",
            PipelineNode {
                recognition: recognition("ColorMatch", json!({ "lower": [0, 0, 0] })),
                action: action("DoNothing", json!({})),
                ..PipelineNode::default()
            },
        )]);

        let issues = validate(&document);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].field, "recognition.upper");
    }

    #[test]
    fn missing_required_action_param_is_error() {
        let document = document_with(vec![(
            "Start",
            PipelineNode {
                recognition: recognition("DirectHit", json!({})),
                action: action("InputText", json!({})),
                ..PipelineNode::default()
            },
        )]);

        let issues = validate(&document);
        assert!(issues
            .iter()
            .any(|item| item.field == "action.input_text" && item.level == IssueLevel::Error));
    }

    #[test]
    fn dangling_next_target_is_error() {
        let document = document_with(vec![(
            "Start",
            PipelineNode {
                recognition: recognition("DirectHit", json!({})),
                action: action("DoNothing", json!({})),
                next: vec![NextEntry::Name("Ghost".to_string())],
                ..PipelineNode::default()
            },
        )]);

        let issues = validate(&document);
        assert!(issues
            .iter()
            .any(|item| item.field == "next" && item.message.contains("Ghost")));
        assert!(has_errors(&issues));
    }

    #[test]
    fn jump_back_target_is_allowed() {
        let document = document_with(vec![(
            "Start",
            PipelineNode {
                recognition: recognition("DirectHit", json!({})),
                action: action("DoNothing", json!({})),
                next: vec![NextEntry::Name("[JumpBack]".to_string())],
                ..PipelineNode::default()
            },
        )]);

        assert!(!has_errors(&validate(&document)));
    }

    #[test]
    fn next_object_form_is_resolved_by_name() {
        let mut entry = serde_json::Map::new();
        entry.insert("name".to_string(), json!("Ghost"));
        let document = document_with(vec![(
            "Start",
            PipelineNode {
                recognition: recognition("DirectHit", json!({})),
                action: action("DoNothing", json!({})),
                next: vec![NextEntry::Object(entry)],
                ..PipelineNode::default()
            },
        )]);

        assert!(validate(&document)
            .iter()
            .any(|item| item.field == "next" && item.message.contains("Ghost")));
    }

    #[test]
    fn direct_hit_click_without_roi_is_fragile_warning() {
        let document = document_with(vec![(
            "Start",
            PipelineNode {
                recognition: recognition("DirectHit", json!({})),
                action: action("Click", json!({})),
                ..PipelineNode::default()
            },
        )]);

        let issues = validate(&document);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].level, IssueLevel::Warning);
        assert!(issues[0].message.contains("换分辨率"));
        // 脆弱只是提示，不算错误
        assert!(!has_errors(&issues));
    }

    #[test]
    fn missing_specs_are_warnings() {
        let document = document_with(vec![("Start", PipelineNode::default())]);
        let issues = validate(&document);
        assert_eq!(issues.len(), 2);
        assert!(issues.iter().all(|item| item.level == IssueLevel::Warning));
    }

    #[test]
    fn empty_document_reports_single_warning() {
        let issues = validate(&PipelineDocument::default());
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].level, IssueLevel::Warning);
        assert_eq!(issues[0].node, "");
        assert!(!has_errors(&issues));
    }

    #[test]
    fn issues_are_sorted_by_node_name() {
        let mut document = PipelineDocument::default();
        for name in ["Zeta", "Alpha", "Mid"] {
            document.nodes.insert(
                name.to_string(),
                PipelineNode {
                    // 故意缺 template，制造 error
                    recognition: recognition("TemplateMatch", json!({})),
                    action: action("DoNothing", json!({})),
                    ..PipelineNode::default()
                },
            );
        }
        let issues = validate(&document);
        let order: Vec<&str> = issues.iter().map(|item| item.node.as_str()).collect();
        assert_eq!(order, vec!["Alpha", "Mid", "Zeta"]);
    }
}
