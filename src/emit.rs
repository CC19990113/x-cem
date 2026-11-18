//! CEM 2.1.0 数据结构与手写 JSON 序列化（不引 serde）。
//!
//! 输出形状与交付规格一致：
//!
//! ```json
//! {
//!   "schemaVersion": "2.1.0",
//!   "modules": [{
//!     "kind": "javascript-module",
//!     "path": "src/button/x-button.ts",
//!     "declarations": [{
//!       "kind": "class",
//!       "name": "XButton",
//!       "tagName": "x-button",
//!       "customElement": true,
//!       "attributes": [],
//!       "events": [],
//!       "slots": [],
//!       "cssProperties": [],
//!       "cssParts": []
//!     }]
//!   }]
//! }
//! ```

use std::fmt::Write as _;

#[derive(Debug, Clone, Default)]
pub struct Attribute {
    pub name: String,
    pub type_text: Option<String>,
    pub description: String,
    pub default: Option<String>,
}

pub type Property = Attribute;

/// 只有 name + description 的结构（events / slots / cssProperties / cssParts 共用）。
#[derive(Debug, Clone, Default)]
pub struct DocumentedItem {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Default)]
pub struct Event {
    pub name: String,
    pub description: String,
    pub type_text: Option<String>,
}

pub type Slot = DocumentedItem;
pub type CssProperty = DocumentedItem;
pub type CssPart = DocumentedItem;

#[derive(Debug, Clone, Default)]
pub struct Declaration {
    pub name: String,
    pub tag_name: Option<String>,
    pub custom_element: bool,
    pub attributes: Vec<Attribute>,
    pub properties: Vec<Property>,
    pub events: Vec<Event>,
    pub slots: Vec<Slot>,
    pub css_properties: Vec<CssProperty>,
    pub css_parts: Vec<CssPart>,
}

#[derive(Debug, Clone, Default)]
pub struct Module {
    pub path: String,
    pub declarations: Vec<Declaration>,
}

#[derive(Debug, Clone)]
pub struct Manifest {
    pub schema_version: String,
    pub modules: Vec<Module>,
}

impl Default for Manifest {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl Manifest {
    pub fn new(modules: Vec<Module>) -> Self {
        Self {
            schema_version: "2.1.0".to_string(),
            modules,
        }
    }

    /// 手写序列化。字段顺序固定，便于 diff 与快照测试。
    pub fn to_json(&self) -> String {
        render_json(&self.schema_version, &self.modules)
    }
}

impl Module {
    /// 单模块便捷序列化（测试 / 基准用），外层仍是完整 Manifest 形状。
    pub fn to_json(&self) -> String {
        render_json("2.1.0", std::slice::from_ref(self))
    }

    /// 只序列化模块对象本身：对应 CEM `modules[]` 里的一项，含
    /// `kind` / `path` / `declarations`，**不带** `schemaVersion` 外壳。
    ///
    /// wasm 导出层用它给单个源文件出结果。
    pub fn to_json_object(&self) -> String {
        let mut out = String::with_capacity(4 * 1024);
        write_module(&mut out, self, 0);
        out.push('\n');
        out
    }
}

fn render_json(schema_version: &str, modules: &[Module]) -> String {
    let mut out = String::with_capacity(16 * 1024);
    out.push_str("{\n  \"schemaVersion\": ");
    write_json_string(&mut out, schema_version);
    out.push_str(",\n  \"modules\": ");
    write_modules(&mut out, modules, 1);
    out.push_str("\n}\n");
    out
}

fn indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str("  ");
    }
}

fn write_modules(out: &mut String, modules: &[Module], level: usize) {
    if modules.is_empty() {
        out.push_str("[]");
        return;
    }
    out.push_str("[\n");
    for (idx, module) in modules.iter().enumerate() {
        indent(out, level + 1);
        write_module(out, module, level + 1);
        if idx + 1 < modules.len() {
            out.push(',');
        }
        out.push('\n');
    }
    indent(out, level);
    out.push(']');
}

fn write_module(out: &mut String, module: &Module, level: usize) {
    out.push_str("{\n");
    indent(out, level + 1);
    out.push_str("\"kind\": \"javascript-module\",\n");
    indent(out, level + 1);
    out.push_str("\"path\": ");
    write_json_string(out, &module.path);
    out.push_str(",\n");
    indent(out, level + 1);
    out.push_str("\"declarations\": ");
    write_declarations(out, &module.declarations, level + 1);
    out.push('\n');
    indent(out, level);
    out.push('}');
}

fn write_declarations(out: &mut String, declarations: &[Declaration], level: usize) {
    if declarations.is_empty() {
        out.push_str("[]");
        return;
    }
    out.push_str("[\n");
    for (idx, declaration) in declarations.iter().enumerate() {
        indent(out, level + 1);
        write_declaration(out, declaration, level + 1);
        if idx + 1 < declarations.len() {
            out.push(',');
        }
        out.push('\n');
    }
    indent(out, level);
    out.push(']');
}

fn write_declaration(out: &mut String, declaration: &Declaration, level: usize) {
    out.push_str("{\n");
    indent(out, level + 1);
    out.push_str("\"kind\": \"class\",\n");
    indent(out, level + 1);
    out.push_str("\"name\": ");
    write_json_string(out, &declaration.name);

    if let Some(tag_name) = &declaration.tag_name {
        out.push_str(",\n");
        indent(out, level + 1);
        out.push_str("\"tagName\": ");
        write_json_string(out, tag_name);
    }

    out.push_str(",\n");
    indent(out, level + 1);
    out.push_str("\"customElement\": ");
    out.push_str(if declaration.custom_element {
        "true"
    } else {
        "false"
    });
    out.push_str(",\n");

    indent(out, level + 1);
    out.push_str("\"attributes\": ");
    write_typed_members(out, &declaration.attributes, level + 1);

    if !declaration.properties.is_empty() {
        out.push_str(",\n");
        indent(out, level + 1);
        out.push_str("\"properties\": ");
        write_typed_members(out, &declaration.properties, level + 1);
    }

    out.push_str(",\n");
    indent(out, level + 1);
    out.push_str("\"events\": ");
    write_events(out, &declaration.events, level + 1);
    out.push_str(",\n");
    indent(out, level + 1);
    out.push_str("\"slots\": ");
    write_documented_items(out, &declaration.slots, level + 1);
    out.push_str(",\n");
    indent(out, level + 1);
    out.push_str("\"cssProperties\": ");
    write_documented_items(out, &declaration.css_properties, level + 1);
    out.push_str(",\n");
    indent(out, level + 1);
    out.push_str("\"cssParts\": ");
    write_documented_items(out, &declaration.css_parts, level + 1);

    out.push('\n');
    indent(out, level);
    out.push('}');
}

fn write_typed_members(out: &mut String, members: &[Attribute], level: usize) {
    if members.is_empty() {
        out.push_str("[]");
        return;
    }
    out.push_str("[\n");
    for (idx, member) in members.iter().enumerate() {
        indent(out, level + 1);
        out.push_str("{\n");
        indent(out, level + 2);
        out.push_str("\"name\": ");
        write_json_string(out, &member.name);
        out.push_str(",\n");

        if let Some(type_text) = &member.type_text {
            indent(out, level + 2);
            out.push_str("\"type\": {\n");
            indent(out, level + 3);
            out.push_str("\"text\": ");
            write_json_string(out, type_text);
            out.push('\n');
            indent(out, level + 2);
            out.push_str("},\n");
        }

        indent(out, level + 2);
        out.push_str("\"description\": ");
        write_json_string(out, &member.description);

        if let Some(default) = &member.default {
            out.push_str(",\n");
            indent(out, level + 2);
            out.push_str("\"default\": ");
            write_json_string(out, default);
        }

        out.push('\n');
        indent(out, level + 1);
        out.push('}');
        if idx + 1 < members.len() {
            out.push(',');
        }
        out.push('\n');
    }
    indent(out, level);
    out.push(']');
}

fn write_events(out: &mut String, events: &[Event], level: usize) {
    if events.is_empty() {
        out.push_str("[]");
        return;
    }
    out.push_str("[\n");
    for (idx, event) in events.iter().enumerate() {
        indent(out, level + 1);
        out.push_str("{\n");
        indent(out, level + 2);
        out.push_str("\"name\": ");
        write_json_string(out, &event.name);
        out.push_str(",\n");

        if let Some(type_text) = &event.type_text {
            indent(out, level + 2);
            out.push_str("\"type\": {\n");
            indent(out, level + 3);
            out.push_str("\"text\": ");
            write_json_string(out, type_text);
            out.push('\n');
            indent(out, level + 2);
            out.push_str("},\n");
        }

        indent(out, level + 2);
        out.push_str("\"description\": ");
        write_json_string(out, &event.description);
        out.push('\n');
        indent(out, level + 1);
        out.push('}');
        if idx + 1 < events.len() {
            out.push(',');
        }
        out.push('\n');
    }
    indent(out, level);
    out.push(']');
}

fn write_documented_items(out: &mut String, items: &[DocumentedItem], level: usize) {
    if items.is_empty() {
        out.push_str("[]");
        return;
    }
    out.push_str("[\n");
    for (idx, item) in items.iter().enumerate() {
        indent(out, level + 1);
        out.push_str("{\n");
        indent(out, level + 2);
        out.push_str("\"name\": ");
        write_json_string(out, &item.name);
        out.push_str(",\n");
        indent(out, level + 2);
        out.push_str("\"description\": ");
        write_json_string(out, &item.description);
        out.push('\n');
        indent(out, level + 1);
        out.push('}');
        if idx + 1 < items.len() {
            out.push(',');
        }
        out.push('\n');
    }
    indent(out, level);
    out.push(']');
}

fn write_json_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}
