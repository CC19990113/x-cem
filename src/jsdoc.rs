//! JSDoc 解析：只认 CEM 需要的标签。
//!
//! 支持的写法：
//!
//! ```text
//! @attr {('default'|'primary')} variant - 视觉变体
//! @attr {('default'|'primary')} [variant='default'] - 视觉变体
//! @attr {boolean} [disabled=false] - 禁用
//! @slot - 按钮文字
//! @slot icon - 前置图标
//! @cssprop --x-button-height - 高度
//! @csspart base - 内部按钮
//! @fires x-button-click - 点击
//! ```
//!
//! 标签后的续行会拼进 description。其余行（组件简介等）当前不输出。

use crate::emit::{Attribute, CssPart, CssProperty, Event, Property, Slot};

#[derive(Debug, Default, Clone)]
pub struct Doc {
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

    let body = raw.strip_prefix("/**").unwrap_or(raw);
    let body = body.strip_suffix("*/").unwrap_or(body);
    for line in body.lines() {
        let line = clean_line(line);
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix('@') {
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
        }
    }

    let mut doc = Doc::default();
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
            entry.description = take_desc(rest);
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
