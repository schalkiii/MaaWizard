use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::Utc;
use inputbot::{KeybdKey, MouseButton, MouseCursor};
use serde::Serialize;
use serde_json::{Map, Value};
use strum::IntoEnumIterator;

use crate::capture::{capture_desktop, crop_roi, save_template};
use crate::pipeline::{ActionSpec, PipelineNode, RecognitionSpec};

/// 按下与松开的曼哈顿距离超过此值，判定为滑动而非点击
const DRAG_THRESHOLD: i32 = 24;
/// 智能模式下默认裁剪的 ROI 边长（像素）
const DEFAULT_ROI_SIZE: u32 = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum RecordMode {
    /// 智能录制：裁剪点击区域生成模板，产出 TemplateMatch 识别节点
    Smart,
    /// 坐标录制：仅记录坐标，产出 DirectHit 节点（抗变性差，仅作兜底）
    Coordinate,
}

impl RecordMode {
    pub fn parse(text: &str) -> Self {
        match text.trim().to_ascii_lowercase().as_str() {
            "coordinate" | "coord" => RecordMode::Coordinate,
            _ => RecordMode::Smart,
        }
    }
}

/// 录制过程中收集到的一个用户操作
#[derive(Debug, Clone, Serialize)]
pub struct RecordedStep {
    pub kind: String,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub end_x: Option<i32>,
    pub end_y: Option<i32>,
    pub duration_ms: Option<i32>,
    pub text: Option<String>,
    pub key: Option<String>,
    /// 智能模式下保存下来的模板文件名（位于资源包 image/ 下）
    pub template: Option<String>,
    pub roi: Option<[i32; 4]>,
}

pub struct RecorderState {
    steps: Arc<Mutex<Vec<RecordedStep>>>,
    recording: Arc<Mutex<bool>>,
    mode: Arc<Mutex<RecordMode>>,
    resource_dir: Arc<Mutex<Option<PathBuf>>>,
    roi_size: Arc<Mutex<u32>>,
    listener_started: Arc<Mutex<bool>>,
}

impl Default for RecorderState {
    fn default() -> Self {
        Self {
            steps: Arc::new(Mutex::new(Vec::new())),
            recording: Arc::new(Mutex::new(false)),
            mode: Arc::new(Mutex::new(RecordMode::Smart)),
            resource_dir: Arc::new(Mutex::new(None)),
            roi_size: Arc::new(Mutex::new(DEFAULT_ROI_SIZE)),
            listener_started: Arc::new(Mutex::new(false)),
        }
    }
}

impl RecorderState {
    pub fn start(&self, mode: RecordMode, resource_dir: &str) -> Result<String, String> {
        *self.mode.lock().map_err(lock_error)? = mode;
        *self.resource_dir.lock().map_err(lock_error)? =
            Some(crate::maa::resolve_existing_path_allow_missing(resource_dir));
        self.steps.lock().map_err(lock_error)?.clear();
        *self.recording.lock().map_err(lock_error)? = true;
        self.ensure_listener();

        Ok(match mode {
            RecordMode::Smart => "已开始智能录制（点击将生成模板匹配节点）".to_string(),
            RecordMode::Coordinate => "已开始坐标录制（抗变性差，建议仅用于固定界面）".to_string(),
        })
    }

    pub fn stop(&self) -> Result<Vec<RecordedStep>, String> {
        *self.recording.lock().map_err(lock_error)? = false;
        Ok(self.steps.lock().map_err(lock_error)?.clone())
    }

    pub fn is_recording(&self) -> bool {
        *self.recording.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// inputbot 的事件循环不可停止，因此只启动一次，用 recording 标志控制是否记录
    fn ensure_listener(&self) {
        let mut started = self.listener_started.lock().unwrap_or_else(|e| e.into_inner());
        if *started {
            return;
        }
        *started = true;

        let steps = Arc::clone(&self.steps);
        let recording = Arc::clone(&self.recording);
        let mode = Arc::clone(&self.mode);
        let resource_dir = Arc::clone(&self.resource_dir);
        let roi_size = Arc::clone(&self.roi_size);

        std::thread::spawn(move || {
            register_bindings(steps, recording, mode, resource_dir, roi_size);
            inputbot::handle_input_events();
        });
    }
}

fn register_bindings(
    steps: Arc<Mutex<Vec<RecordedStep>>>,
    recording: Arc<Mutex<bool>>,
    mode: Arc<Mutex<RecordMode>>,
    resource_dir: Arc<Mutex<Option<PathBuf>>>,
    roi_size: Arc<Mutex<u32>>,
) {
    // 鼠标左键：按下即记录起点，轮询等待松开以区分"点击"与"拖拽滑动"
    {
        let steps = Arc::clone(&steps);
        let recording = Arc::clone(&recording);
        let mode = Arc::clone(&mode);
        let resource_dir = Arc::clone(&resource_dir);
        let roi_size = Arc::clone(&roi_size);

        MouseButton::LeftButton.bind(move || {
            if !is_enabled(&recording) {
                return;
            }
            let (start_x, start_y) = MouseCursor::pos();
            let started_at = Instant::now();

            // bind 要求闭包实现 Fn（可重复调用），因此先克隆 Arc 再移入线程
            let steps = Arc::clone(&steps);
            let mode = Arc::clone(&mode);
            let resource_dir = Arc::clone(&resource_dir);
            let roi_size = Arc::clone(&roi_size);

            std::thread::spawn(move || {
                while MouseButton::LeftButton.is_pressed() {
                    std::thread::sleep(Duration::from_millis(8));
                }
                let (end_x, end_y) = MouseCursor::pos();
                let duration_ms = started_at.elapsed().as_millis() as i32;
                let distance = (end_x - start_x).abs() + (end_y - start_y).abs();

                if distance > DRAG_THRESHOLD {
                    push_step(
                        &steps,
                        RecordedStep {
                            kind: "Swipe".to_string(),
                            x: Some(start_x),
                            y: Some(start_y),
                            end_x: Some(end_x),
                            end_y: Some(end_y),
                            duration_ms: Some(duration_ms.max(1)),
                            text: None,
                            key: None,
                            template: None,
                            roi: None,
                        },
                    );
                } else {
                    handle_click(
                        &steps,
                        &mode,
                        &resource_dir,
                        &roi_size,
                        start_x,
                        start_y,
                    );
                }
            });
        });
    }

    // 键盘：绑定所有按键，可打印字符累积为文本输入，功能键记录为按键
    for key in KeybdKey::iter() {
        let steps = Arc::clone(&steps);
        let recording = Arc::clone(&recording);
        key.bind(move || {
            if !is_enabled(&recording) {
                return;
            }
            let name = format!("{:?}", key);
            // 修饰键自身不单独成步
            if name.contains("Shift") || name.contains("Control") || name.contains("Alt") {
                return;
            }
            let uppercase = KeybdKey::LShiftKey.is_pressed() || KeybdKey::RShiftKey.is_pressed();

            let step = match key_to_char(&name, uppercase) {
                Some(character) => RecordedStep {
                    kind: "Text".to_string(),
                    x: None,
                    y: None,
                    end_x: None,
                    end_y: None,
                    duration_ms: None,
                    text: Some(character.to_string()),
                    key: None,
                    template: None,
                    roi: None,
                },
                None => RecordedStep {
                    kind: "Key".to_string(),
                    x: None,
                    y: None,
                    end_x: None,
                    end_y: None,
                    duration_ms: None,
                    text: None,
                    key: Some(name.clone()),
                    template: None,
                    roi: None,
                },
            };
            push_step(&steps, step);
        });
    }
}

/// 智能模式下截取点击区域作为模板；坐标模式下只记录坐标
fn handle_click(
    steps: &Arc<Mutex<Vec<RecordedStep>>>,
    mode: &Arc<Mutex<RecordMode>>,
    resource_dir: &Arc<Mutex<Option<PathBuf>>>,
    roi_size: &Arc<Mutex<u32>>,
    x: i32,
    y: i32,
) {
    let mut template = None;
    let mut roi = None;

    let smart = matches!(
        *mode.lock().unwrap_or_else(|e| e.into_inner()),
        RecordMode::Smart
    );

    if smart {
        if let Ok(screen) = capture_desktop() {
            let size = *roi_size.lock().unwrap_or_else(|e| e.into_inner());
            if let Some((cropped, rect)) = crop_roi(&screen, x, y, size) {
                let dir_guard = resource_dir.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(dir) = dir_guard.as_ref() {
                    let file_name = format!("rec_{}.png", Utc::now().timestamp_millis());
                    let path = dir.join("image").join(&file_name);
                    if save_template(&cropped, &path).is_ok() {
                        template = Some(file_name);
                        roi = Some(rect);
                    }
                }
            }
        }
    }

    push_step(
        steps,
        RecordedStep {
            kind: "Click".to_string(),
            x: Some(x),
            y: Some(y),
            end_x: None,
            end_y: None,
            duration_ms: None,
            text: None,
            key: None,
            template,
            roi,
        },
    );
}

fn push_step(steps: &Arc<Mutex<Vec<RecordedStep>>>, step: RecordedStep) {
    if let Ok(mut guard) = steps.lock() {
        guard.push(step);
    }
}

fn is_enabled(recording: &Arc<Mutex<bool>>) -> bool {
    *recording.lock().unwrap_or_else(|e| e.into_inner())
}

/// 将录制步骤转换为 Pipeline 节点序列（尚未命名、尚未串联）。
/// 连续的字符输入会合并成单个 InputText 节点。
pub fn steps_to_nodes(steps: &[RecordedStep]) -> Vec<PipelineNode> {
    let mut nodes: Vec<PipelineNode> = Vec::new();
    let mut pending_text = String::new();

    for step in steps {
        match step.kind.as_str() {
            // 连续的字符输入先累积，遇到其它操作时再落成一个 InputText 节点
            "Text" => {
                if let Some(text) = &step.text {
                    pending_text.push_str(text);
                }
                continue;
            }
            "Click" => {
                flush_text(&mut nodes, &mut pending_text);
                nodes.push(build_click_node(step));
            }
            "Swipe" => {
                flush_text(&mut nodes, &mut pending_text);
                nodes.push(build_swipe_node(step));
            }
            "Key" => {
                flush_text(&mut nodes, &mut pending_text);
                nodes.push(build_key_node(step));
            }
            _ => {}
        }
    }

    flush_text(&mut nodes, &mut pending_text);
    nodes
}

/// 把累积的文本落成一个 InputText 节点（空文本则忽略）
fn flush_text(nodes: &mut Vec<PipelineNode>, pending_text: &mut String) {
    if let Some(node) = build_text_node(pending_text) {
        nodes.push(node);
    }
    pending_text.clear();
}

fn build_text_node(text: &str) -> Option<PipelineNode> {
    if text.is_empty() {
        return None;
    }
    let mut param = Map::new();
    param.insert("input_text".to_string(), Value::String(text.to_string()));
    Some(node_with(
        RecognitionSpec {
            kind: "DirectHit".to_string(),
            param: Map::new(),
        },
        ActionSpec {
            kind: "InputText".to_string(),
            param,
        },
    ))
}

fn build_click_node(step: &RecordedStep) -> PipelineNode {
    match &step.template {
        Some(template) => {
            // 智能识别：以模板图定位，点击识别命中区域（动作默认作用于 hitbox）
            let mut param = Map::new();
            param.insert("template".to_string(), Value::String(template.clone()));
            node_with(
                RecognitionSpec {
                    kind: "TemplateMatch".to_string(),
                    param,
                },
                ActionSpec {
                    kind: "Click".to_string(),
                    param: Map::new(),
                },
            )
        }
        None => {
            // 坐标兜底：DirectHit 定位到具体坐标
            let x = step.x.unwrap_or(0);
            let y = step.y.unwrap_or(0);
            let mut reco_param = Map::new();
            reco_param.insert(
                "roi".to_string(),
                Value::Array(vec![
                    Value::from(x),
                    Value::from(y),
                    Value::from(1),
                    Value::from(1),
                ]),
            );
            node_with(
                RecognitionSpec {
                    kind: "DirectHit".to_string(),
                    param: reco_param,
                },
                ActionSpec {
                    kind: "Click".to_string(),
                    param: Map::new(),
                },
            )
        }
    }
}

fn build_swipe_node(step: &RecordedStep) -> PipelineNode {
    let mut param = Map::new();
    param.insert(
        "begin".to_string(),
        Value::Array(vec![
            Value::from(step.x.unwrap_or(0)),
            Value::from(step.y.unwrap_or(0)),
        ]),
    );
    param.insert(
        "end".to_string(),
        Value::Array(vec![
            Value::from(step.end_x.unwrap_or(0)),
            Value::from(step.end_y.unwrap_or(0)),
        ]),
    );
    param.insert(
        "duration".to_string(),
        Value::from(step.duration_ms.unwrap_or(300)),
    );

    node_with(
        RecognitionSpec {
            kind: "DirectHit".to_string(),
            param: Map::new(),
        },
        ActionSpec {
            kind: "Swipe".to_string(),
            param,
        },
    )
}

fn build_key_node(step: &RecordedStep) -> PipelineNode {
    let name = step.key.clone().unwrap_or_default();
    let mut param = Map::new();
    param.insert("key".to_string(), Value::from(key_to_vk(&name).unwrap_or(0)));

    let mut node = node_with(
        RecognitionSpec {
            kind: "DirectHit".to_string(),
            param: Map::new(),
        },
        ActionSpec {
            kind: "ClickKey".to_string(),
            param,
        },
    );
    // 保留按键名便于用户核对/修改
    node.extra
        .insert("key_name".to_string(), Value::String(name));
    node
}

fn node_with(recognition: RecognitionSpec, action: ActionSpec) -> PipelineNode {
    let mut node = PipelineNode::with_defaults();
    node.recognition = Some(recognition);
    node.action = Some(action);
    node
}

/// inputbot 的按键变体名（如 "AKey"、"Numrow1Key"）转可打印字符
fn key_to_char(name: &str, uppercase: bool) -> Option<char> {
    let character = match name {
        "SpaceKey" => ' ',
        "MinusKey" => '-',
        "EqualKey" => '=',
        "CommaKey" => ',',
        "PeriodKey" => '.',
        "SlashKey" => '/',
        "SemicolonKey" => ';',
        "QuoteKey" => '\'',
        "LBracketKey" => '[',
        "RBracketKey" => ']',
        "BackslashKey" => '\\',
        "GraveKey" => '`',
        other => {
            if other.len() == 4 && other.ends_with("Key") {
                let first = other.chars().next()?;
                if first.is_ascii_alphabetic() {
                    first.to_ascii_lowercase()
                } else {
                    return None;
                }
            } else {
                other
                    .strip_prefix("Numrow")
                    .and_then(|rest| rest.strip_suffix("Key"))?
                    .chars()
                    .next()?
            }
        }
    };

    Some(if uppercase {
        character.to_ascii_uppercase()
    } else {
        character
    })
}

/// 按键名转 Win32 虚拟键码，供 ClickKey 使用
fn key_to_vk(name: &str) -> Option<i32> {
    let known = match name {
        "EnterKey" => Some(13),
        "TabKey" => Some(9),
        "EscapeKey" | "EscKey" => Some(27),
        "BackspaceKey" => Some(8),
        "SpaceKey" => Some(32),
        "DeleteKey" => Some(46),
        "HomeKey" => Some(36),
        "EndKey" => Some(35),
        "PageUpKey" => Some(33),
        "PageDownKey" => Some(34),
        "UpKey" => Some(38),
        "DownKey" => Some(40),
        "LeftKey" => Some(37),
        "RightKey" => Some(39),
        _ => None,
    };
    if let Some(code) = known {
        return Some(code);
    }

    if let Some(character) = key_to_char(name, false) {
        return Some(character.to_ascii_uppercase() as i32);
    }

    // F1~F12 → VK_F1(112) 起
    if let Some(number) = name
        .strip_prefix('F')
        .and_then(|rest| rest.strip_suffix("Key"))
    {
        if let Ok(value) = number.parse::<i32>() {
            if (1..=12).contains(&value) {
                return Some(111 + value);
            }
        }
    }

    None
}

fn lock_error<T>(_value: T) -> String {
    "录制状态锁损坏".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn click_step(x: i32, y: i32, template: Option<&str>) -> RecordedStep {
        RecordedStep {
            kind: "Click".to_string(),
            x: Some(x),
            y: Some(y),
            end_x: None,
            end_y: None,
            duration_ms: None,
            text: None,
            key: None,
            template: template.map(String::from),
            roi: None,
        }
    }

    fn text_step(character: char) -> RecordedStep {
        RecordedStep {
            kind: "Text".to_string(),
            x: None,
            y: None,
            end_x: None,
            end_y: None,
            duration_ms: None,
            text: Some(character.to_string()),
            key: None,
            template: None,
            roi: None,
        }
    }

    fn swipe_step() -> RecordedStep {
        RecordedStep {
            kind: "Swipe".to_string(),
            x: Some(0),
            y: Some(0),
            end_x: Some(300),
            end_y: Some(400),
            duration_ms: Some(500),
            text: None,
            key: None,
            template: None,
            roi: None,
        }
    }

    fn key_step(name: &str) -> RecordedStep {
        RecordedStep {
            kind: "Key".to_string(),
            x: None,
            y: None,
            end_x: None,
            end_y: None,
            duration_ms: None,
            text: None,
            key: Some(name.to_string()),
            template: None,
            roi: None,
        }
    }

    #[test]
    fn click_with_template_becomes_template_match_and_click() {
        let nodes = steps_to_nodes(&[click_step(100, 200, Some("rec_1.png"))]);
        assert_eq!(nodes.len(), 1);
        let recognition = nodes[0].recognition.as_ref().unwrap();
        assert_eq!(recognition.kind, "TemplateMatch");
        assert_eq!(recognition.param.get("template").unwrap(), "rec_1.png");
        assert_eq!(nodes[0].action.as_ref().unwrap().kind, "Click");
    }

    #[test]
    fn click_without_template_becomes_direct_hit_with_roi() {
        let nodes = steps_to_nodes(&[click_step(100, 200, None)]);
        let recognition = nodes[0].recognition.as_ref().unwrap();
        assert_eq!(recognition.kind, "DirectHit");
        let roi = recognition.param.get("roi").unwrap().as_array().unwrap();
        assert_eq!(roi[0], 100);
        assert_eq!(roi[1], 200);
    }

    #[test]
    fn consecutive_text_merges_into_single_input_text() {
        let nodes = steps_to_nodes(&[text_step('a'), text_step('b'), text_step('c')]);
        assert_eq!(nodes.len(), 1);
        let action = nodes[0].action.as_ref().unwrap();
        assert_eq!(action.kind, "InputText");
        assert_eq!(action.param.get("input_text").unwrap(), "abc");
    }

    #[test]
    fn text_between_clicks_yields_separate_nodes() {
        let steps = vec![click_step(0, 0, None), text_step('x'), click_step(1, 1, None)];
        let nodes = steps_to_nodes(&steps);
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[1].action.as_ref().unwrap().kind, "InputText");
    }

    #[test]
    fn swipe_becomes_swipe_action_with_coordinates() {
        let nodes = steps_to_nodes(&[swipe_step()]);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].action.as_ref().unwrap().kind, "Swipe");
        let param = &nodes[0].action.as_ref().unwrap().param;
        assert_eq!(param.get("begin").unwrap(), &json!([0, 0]));
        assert_eq!(param.get("end").unwrap(), &json!([300, 400]));
        assert_eq!(param.get("duration").unwrap().as_i64(), Some(500));
    }

    #[test]
    fn key_becomes_click_key_with_virtual_key_code() {
        let nodes = steps_to_nodes(&[key_step("EnterKey")]);
        let action = nodes[0].action.as_ref().unwrap();
        assert_eq!(action.kind, "ClickKey");
        assert_eq!(action.param.get("key").unwrap().as_i64(), Some(13));
        // 原始按键名写入 extra，便于人工核对
        assert_eq!(nodes[0].extra.get("key_name").unwrap(), "EnterKey");
    }

    #[test]
    fn empty_steps_produce_no_nodes() {
        assert!(steps_to_nodes(&[]).is_empty());
    }

    #[test]
    fn key_to_char_maps_printable_keys() {
        assert_eq!(key_to_char("AKey", false), Some('a'));
        assert_eq!(key_to_char("AKey", true), Some('A'));
        assert_eq!(key_to_char("SpaceKey", false), Some(' '));
        assert_eq!(key_to_char("Numrow1Key", false), Some('1'));
        assert_eq!(key_to_char("EnterKey", false), None);
    }

    #[test]
    fn key_to_vk_maps_known_keys() {
        assert_eq!(key_to_vk("EnterKey"), Some(13));
        assert_eq!(key_to_vk("AKey"), Some(65));
        assert_eq!(key_to_vk("F1Key"), Some(112));
        assert_eq!(key_to_vk("SpaceKey"), Some(32));
        assert_eq!(key_to_vk("UnknownKey"), None);
    }
}
