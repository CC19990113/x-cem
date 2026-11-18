//! JSDoc 解析：只认 CEM 需要的标签。
//!
//! 支持的写法：
//!
//! ```text
//! @attr {('default'|'primary')} variant - 视觉变体
//! @attr {('default'|'primary')} [variant='default'] - 视觉变体
//! @attr {('default'|'primary')} variant = default - 视觉变体
//! @attr {('default'|'primary')} variant - 视觉变体。Default: "default".
//! @attr {boolean} [disabled=false] - 禁用
//! @slot - 按钮文字
//! @slot icon - 前置图标
//! @cssprop --x-button-height - 高度
//! @csspart base - 内部按钮
//! @fires x-button-click - 点击
//! ```
//!
//! 默认值解析优先级（高 → 低）：
//!
//! 1. JSDoc 显式默认值：`[name=value]` 或 `name = value`；
//! 2. 描述文本里的 `Default: value` / `Default: "value"`（大小写不敏感，引号可选）。
//!
//! 两条都取不到就不输出 `default` 字段——布尔属性不会被补成 `false`。
//!
//! 标签后的续行会拼进 description。JSDoc 首段（第一个空行之前、任何标签之前的
//! 自然段）作为组件描述输出，为空则不输出。

use crate::emit::{Attribute, CssPart, CssProperty, Event, Property, Slot};

#[derive(Debug, Default, Clone)]
pub struct Doc {
    /// JSDoc 首段（第一个空行 / 第一个标签之前），可能为空字符串。
    pub description: String,
    pub attributes: Vec<Attribute>,
    pub properties: Vec<Property>,
    pub events: Vec<Event>,
    pub slots: Vec<Slot>,
    pub css_properties: Vec<CssProperty>,
    pub css_parts: Vec<CssPart>,
}

impl Doc {
    /// 是否出现了任意一个 CEM 标签（用来判断一个未注册的 class 要不要当成组件输出）。
    pub fn has_cem_tags(&self) -> bool {
        !self.attributes.is_empty()
            || !self.properties.is_empty()
            || !self.events.is_empty()
            || !self.slots.is_empty()
            || !self.css_properties.is_empty()
            || !self.css_parts.is_empty()
    }
}

#[derive(Debug)]
struct Entry {
    tag: String,
    type_text: Option<String>,
    name: String,
    default: Option<String>,
    description: String,
}

pub fn parse(raw: &str) -> Doc {
    let mut entries: Vec<Entry> = Vec::new();
    let mut current: Option<usize> = None;
    let mut summary = String::new();
    // 首段只收「第一个空行之前」且「任何标签之前」的文本行。
    let mut in_summary = true;
    // `/**` 后面的换行会先产生一个空行，那不算段落结束。
    let mut started = false;

    let body = raw.strip_prefix("/**").unwrap_or(raw);
    let body = body.strip_suffix("*/").unwrap_or(body);
    for line in body.lines() {
        let line = clean_line(line);
        if line.is_empty() {
            // 首段开始后的第一个空行即段落结束。
            if started {
                in_summary = false;
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix('@') {
            in_summary = false;
            current = parse_tag(rest).map(|entry| {
                entries.push(entry);
                entries.len() - 1
            });
        } else if let Some(idx) = current {
            let entry = &mut entries[idx];
            if !entry.description.is_empty() {
                entry.description.push('\n');
            }
            entry.description.push_str(line);
        } else if in_summary {
            started = true;
            if !summary.is_empty() {
                summary.push(' ');
            }
            summary.push_str(line);
        }
    }

    let mut doc = Doc {
        description: summary,
        ..Doc::default()
    };
    for entry in entries {
        let tag = entry.tag.as_str();
        match tag {
            "attr" | "attribute" => doc.attributes.push(Attribute {
                name: entry.name,
                type_text: entry.type_text,
                description: entry.description,
                default: entry.default,
            }),
            "prop" | "property" => doc.properties.push(Property {
                name: entry.name,
                type_text: entry.type_text,
                description: entry.description,
                default: entry.default,
            }),
            "slot" => doc.slots.push(Slot {
                name: entry.name,
                description: entry.description,
            }),
            "cssprop" | "cssproperty" => doc.css_properties.push(CssProperty {
                name: entry.name,
                description: entry.description,
            }),
            "csspart" => doc.css_parts.push(CssPart {
                name: entry.name,
                description: entry.description,
            }),
            "fires" | "fire" | "event" => doc.events.push(Event {
                name: entry.name,
                description: entry.description,
                type_text: entry.type_text,
            }),
            _ => {}
        }
    }
    doc
}

/// 去掉每行开头的空白与 `*`。
fn clean_line(line: &str) -> &str {
    let line = line.trim();
    let line = line.strip_prefix('*').unwrap_or(line);
    line.trim()
}

fn parse_tag(rest: &str) -> Option<Entry> {
    let tag_end = rest
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(rest.len());
    if tag_end == 0 {
        return None;
    }
    let tag = rest[..tag_end].to_ascii_lowercase();
    let rest = rest[tag_end..].trim_start();

    let mut entry = Entry {
        tag: tag.clone(),
        type_text: None,
        name: String::new(),
        default: None,
        description: String::new(),
    };

    match tag.as_str() {
        "attr" | "attribute" | "prop" | "property" => {
            let (ty, rest) = take_type(rest);
            entry.type_text = ty.map(str::to_string);
            let (name, default, rest) = take_member(rest);
            entry.name = name;
            entry.default = default;
            // 显式写法 `name = value` 优先于描述文本里的 `Default: value`。
            let rest = if entry.default.is_none() {
                match take_assignment(rest) {
                    Some((value, tail)) => {
                        entry.default = Some(value);
                        tail
                    }
                    None => rest,
                }
            } else {
                rest
            };
            entry.description = take_desc(rest);
            // 描述文本兜底：Default: "md" / Default: md（大小写不敏感，引号可选）。
            if entry.default.is_none() {
                entry.default = default_from_text(&entry.description);
            }
        }
        "slot" => {
            let (_ty, rest) = take_type(rest);
            if rest.trim_start().starts_with('-') {
                entry.description = take_desc(rest);
            } else {
                let (name, _default, rest) = take_member(rest);
                entry.name = name;
                entry.description = take_desc(rest);
            }
        }
        "cssprop" | "cssproperty" | "csspart" => {
            let (_ty, rest) = take_type(rest);
            let (name, _default, rest) = take_member(rest);
            entry.name = name;
            entry.description = take_desc(rest);
        }
        "fires" | "fire" | "event" => {
            let (ty, rest) = take_type(rest);
            entry.type_text = ty.map(str::to_string);
            let (name, _default, rest) = take_member(rest);
            entry.name = name;
            entry.description = take_desc(rest);
        }
        _ => return None,
    }
    Some(entry)
}

/// 取开头的 `{Type}`（允许嵌套花括号），返回 (类型文本, 剩余)。
fn take_type(s: &str) -> (Option<&str>, &str) {
    if !s.starts_with('{') {
        return (None, s);
    }
    let mut depth = 0usize;
    for (idx, b) in s.bytes().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return (Some(s[1..idx].trim()), s[idx + 1..].trim_start());
                }
            }
            _ => {}
        }
    }
    (None, s)
}

/// 取成员名：支持 `[name='default']` 形式，返回 (名字, 默认值, 剩余)。
fn take_member(s: &str) -> (String, Option<String>, &str) {
    let s = s.trim_start();
    if let Some(inner_start) = s.strip_prefix('[') {
        if let Some(close) = inner_start.find(']') {
            let inner = &inner_start[..close];
            let after = &inner_start[close + 1..];
            let (name, default) = match inner.split_once('=') {
                Some((name, default)) => (name.trim(), Some(unquote(default.trim()))),
                None => (inner.trim(), None),
            };
            return (unquote(name), default, after);
        }
    }

    let mut end = 0usize;
    for (idx, c) in s.char_indices() {
        if c.is_alphanumeric() || matches!(c, '_' | '$' | '-' | '.') {
            end = idx + c.len_utf8();
        } else {
            break;
        }
    }
    (unquote(&s[..end]), None, &s[end..])
}

/// 取 ` - 描述`；`@slot - 文字` 这种无名写法直接吃 `-` 后面的内容。
fn take_desc(s: &str) -> String {
    let s = s.trim();
    if s.is_empty() || s == "-" {
        return String::new();
    }
    if let Some(rest) = s.strip_prefix("- ") {
        return rest.trim().to_string();
    }
    if let Some(pos) = s.find(" - ") {
        return s[pos + 3..].trim().to_string();
    }
    String::new()
}

/// 取 `= value` 形式的默认值，返回 (值, 剩余)。
///
/// 支持 `size = md - 说明`、`size = "a b" - 说明`；`=` 后为空则视为没写。
fn take_assignment(s: &str) -> Option<(String, &str)> {
    let rest = s.trim_start().strip_prefix('=')?;
    let rest = rest.trim_start();
    let first = rest.as_bytes().first().copied()?;
    let (value, tail) = if first == b'"' || first == b'\'' {
        let quote = first as char;
        let inner = rest[1..].find(quote)?;
        (unquote(&rest[..inner + 2]), &rest[inner + 2..])
    } else {
        let end = rest
            .char_indices()
            .find(|(_, c)| c.is_whitespace())
            .map(|(idx, _)| idx)
            .unwrap_or(rest.len());
        (unquote(&rest[..end]), &rest[end..])
    };
    let value = value.trim().to_string();
    if value.is_empty() {
        return None;
    }
    Some((value, tail))
}

/// 从描述文本里抠 `Default: value`（大小写不敏感，引号可选）。
///
/// 只认独立出现的 `default:`，避免误吃 `subdefault:` 这类词。
fn default_from_text(description: &str) -> Option<String> {
    let lower = description.to_ascii_lowercase();
    let needle = "default:";
    let mut start = 0usize;
    while let Some(rel) = lower[start..].find(needle) {
        let idx = start + rel;
        let boundary = idx == 0 || !description.as_bytes()[idx - 1].is_ascii_alphanumeric();
        if boundary {
            let after = description[idx + needle.len()..].trim_start();
            if let Some(value) = take_text_value(after) {
                return Some(value);
            }
        }
        start = idx + needle.len();
    }
    None
}

/// 取描述里的一个值 token：引号内整段优先，否则到空白为止并去掉尾部标点。
fn take_text_value(s: &str) -> Option<String> {
    let first = s.as_bytes().first().copied()?;
    let raw = if first == b'"' || first == b'\'' {
        let quote = first as char;
        let rel = s[1..].find(quote)?;
        s[1..rel + 1].to_string()
    } else {
        let end = s
            .char_indices()
            .find(|(_, c)| c.is_whitespace())
            .map(|(idx, _)| idx)
            .unwrap_or(s.len());
        s[..end]
            .trim_end_matches(['.', ',', ';', ':', ')', ']', '}'])
            .to_string()
    };
    let value = raw.trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    let bytes = s.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'\'' && last == b'\'') || (first == b'"' && last == b'"') {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}
