//! 集成测试：解析 fixture 断言 CEM 结构，并用诱饵代码验证词法器的跳过逻辑。

use x_cem::emit::Module;
use x_cem::{analyze_many, analyze_source, manifest_for_source};

const FIXTURE: &str = include_str!("fixtures/x-button.ts");
const FIXTURE_PATH: &str = "tests/fixtures/x-button.ts";

fn fixture_module() -> Module {
    manifest_for_source(FIXTURE_PATH, FIXTURE).expect("fixture 应产出至少一个模块声明")
}

#[test]
fn fixture_parses_into_one_custom_element() {
    let module = fixture_module();
    assert_eq!(module.path, FIXTURE_PATH);
    assert_eq!(
        module.declarations.len(),
        1,
        "fixture 里只应有一个自定义元素"
    );

    let declaration = &module.declarations[0];
    assert_eq!(declaration.name, "XButton");
    assert_eq!(declaration.tag_name.as_deref(), Some("x-button"));
    assert!(declaration.custom_element);
}

#[test]
fn fixture_attributes_are_complete() {
    let module = fixture_module();
    let attributes = &module.declarations[0].attributes;
    let names: Vec<&str> = attributes.iter().map(|attr| attr.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["variant", "size", "disabled", "loading", "type"]
    );

    let variant = &attributes[0];
    assert_eq!(
        variant.type_text.as_deref(),
        Some("('default'|'primary'|'danger'|'ghost')")
    );
    assert_eq!(variant.description, "视觉变体");
    assert_eq!(variant.default.as_deref(), Some("default"));

    let disabled = &attributes[2];
    assert_eq!(disabled.type_text.as_deref(), Some("boolean"));
    assert_eq!(disabled.description, "禁用状态");
    assert_eq!(disabled.default.as_deref(), Some("false"));

    let size = &attributes[1];
    assert_eq!(size.type_text.as_deref(), Some("('sm'|'md'|'lg')"));
    assert_eq!(size.description, "尺寸。Default: \"md\"");
    assert_eq!(
        size.default.as_deref(),
        Some("md"),
        "描述里的 `Default: \"md\"` 要解析成去引号的裸值"
    );

    // 源码里没写默认值的布尔属性：不许凭空造 false。
    let loading = &attributes[3];
    assert_eq!(loading.type_text.as_deref(), Some("boolean"));
    assert!(
        loading.default.is_none(),
        "无默认值的布尔属性不该有 default: {:?}",
        loading.default
    );

    assert_eq!(
        attributes[4].type_text.as_deref(),
        Some("('button'|'submit'|'reset')")
    );
    assert_eq!(attributes[4].default.as_deref(), Some("button"));
}

#[test]
fn fixture_events_slots_css_props_and_parts_are_complete() {
    let module = fixture_module();
    let declaration = &module.declarations[0];

    assert_eq!(declaration.events.len(), 1);
    assert_eq!(declaration.events[0].name, "x-button-click");
    assert_eq!(
        declaration.events[0].description,
        "点击时触发，detail 为 { originalEvent }"
    );

    let slots: Vec<(&str, &str)> = declaration
        .slots
        .iter()
        .map(|slot| (slot.name.as_str(), slot.description.as_str()))
        .collect();
    assert_eq!(slots, vec![("", "按钮文字"), ("icon", "前置图标")]);

    let css_properties: Vec<&str> = declaration
        .css_properties
        .iter()
        .map(|property| property.name.as_str())
        .collect();
    assert_eq!(
        css_properties,
        vec!["--x-button-height", "--x-button-radius"]
    );
    assert_eq!(declaration.css_properties[0].description, "高度");

    assert_eq!(declaration.css_parts.len(), 1);
    assert_eq!(declaration.css_parts[0].name, "base");
    assert_eq!(declaration.css_parts[0].description, "内部按钮");
}

#[test]
fn fixture_json_has_expected_shape() {
    let json = fixture_module().to_json();
    assert!(json.starts_with("{\n  \"schemaVersion\": \"2.1.0\",\n  \"modules\": ["));
    assert!(json.contains("\"kind\": \"javascript-module\""));
    assert!(json.contains(&format!("\"path\": \"{FIXTURE_PATH}\"")));
    assert!(json.contains("\"kind\": \"class\""));
    assert!(json.contains("\"name\": \"XButton\""));
    assert!(json.contains("\"tagName\": \"x-button\""));
    assert!(json.contains("\"customElement\": true"));
    for key in ["attributes", "events", "slots", "cssProperties", "cssParts"] {
        assert!(json.contains(&format!("\"{key}\": [")), "缺少字段 {key}");
    }
    assert!(json.contains("\"default\": \"default\""));
    assert!(json.contains("\"description\": \"视觉变体\""));
    assert!(json.ends_with("}\n"));
    assert_json_valid(&json);
}

#[test]
fn skips_strings_comments_templates_and_regex() {
    let src = r#"
// customElements.define('x-line-comment', FakeA);
/* customElements.define('x-block-comment', FakeB); */
const doubleQuoted = "customElements.define('x-double', FakeC)";
const singleQuoted = 'customElements.define("x-single", FakeD)';
const template = `customElements.define('x-template', FakeE)`;
const nested = `${`${"customElements.define('x-nested', FakeF)"}`}`;
const regex = /customElements\.define\('x-regex', FakeG\)/;
const charClass = /[/]class FakeH extends HTMLElement/;
const arithmetic = 10 / 2 / 5;
const classInString = "class FakeI extends HTMLElement {}";

export class RealOne extends HTMLElement {}
customElements.define('x-real', RealOne);
"#;
    let declarations = analyze_source(src);
    assert_eq!(
        declarations.len(),
        1,
        "只应识别出 RealOne，实际: {declarations:#?}"
    );
    assert_eq!(declarations[0].name, "RealOne");
    assert_eq!(declarations[0].tag_name.as_deref(), Some("x-real"));
}

#[test]
fn define_supports_double_quotes_backticks_and_multiline() {
    let src = r#"
export class Aaa extends HTMLElement {}
customElements.define(
  "x-aaa",
  Aaa,
);

export class Bbb extends HTMLElement {}
customElements.define(
  `x-bbb`,
  Bbb
);
"#;
    let declarations = analyze_source(src);
    assert_eq!(declarations.len(), 2);
    assert_eq!(declarations[0].name, "Aaa");
    assert_eq!(declarations[0].tag_name.as_deref(), Some("x-aaa"));
    assert_eq!(declarations[1].name, "Bbb");
    assert_eq!(declarations[1].tag_name.as_deref(), Some("x-bbb"));
}

#[test]
fn jsdoc_tags_parse_without_default_and_with_multiline_description() {
    let src = r#"
/**
 * @attr {('a'|'b')} variant - 变体
 * @slot name - 插槽
 * @cssprop --x-h - 高度
 * @csspart base - 部件
 * @fires x-ping - 事件
 *   第二行说明
 */
class Plain extends HTMLElement {}
"#;
    let declarations = analyze_source(src);
    assert_eq!(declarations.len(), 1);
    let declaration = &declarations[0];
    assert!(declaration.tag_name.is_none());
    assert!(!declaration.custom_element);
    assert_eq!(declaration.attributes[0].name, "variant");
    assert!(declaration.attributes[0].default.is_none());
    assert_eq!(declaration.slots[0].name, "name");
    assert_eq!(declaration.css_properties[0].name, "--x-h");
    assert_eq!(declaration.css_parts[0].name, "base");
    assert_eq!(declaration.events[0].description, "事件\n第二行说明");
}

#[test]
fn jsdoc_must_be_adjacent_to_class() {
    let src = "/** @fires x-a-click - 点击 */\nconst unrelated = 1;\nexport class Aaa extends HTMLElement {}\n";
    let declarations = analyze_source(src);
    assert_eq!(declarations.len(), 1);
    assert!(
        declarations[0].events.is_empty(),
        "隔了其它语句的 JSDoc 不应挂到 class 上"
    );
}

#[test]
fn non_element_classes_are_not_emitted() {
    let src = "class Helper { handle() { return 1; } }\nexport function foo() {}\n";
    assert!(analyze_source(src).is_empty());
}

#[test]
fn non_registered_element_subclass_is_emitted_without_tag() {
    let src = "export class Fancy extends HTMLElement {}\n";
    let declarations = analyze_source(src);
    assert_eq!(declarations.len(), 1);
    assert!(declarations[0].tag_name.is_none());
    assert!(!declarations[0].custom_element);
}

#[test]
fn json_escapes_quotes_backslashes_and_newlines() {
    let src = r#"
/**
 * @fires x-q - 引号 " 与反斜杠 \ 以及换行续行
 * 第二行
 */
class Qq extends HTMLElement {}
customElements.define('x-q', Qq);
"#;
    let json = manifest_for_source("q.ts", src)
        .expect("应有声明")
        .to_json();
    assert!(
        json.contains(r#""description": "引号 \" 与反斜杠 \\ 以及换行续行\n第二行""#),
        "转义不正确: {json}"
    );
    assert_json_valid(&json);
}

#[test]
fn t2_style_jsdoc_without_defaults_is_supported() {
    let src = r#"
/**
 * @attr {string} variant - 视觉变体
 * @attr {boolean} disabled - 禁用
 * @slot - 按钮文字
 * @cssprop --x-button-height - 高度
 * @csspart base - 内部按钮
 * @fires x-button-click - 点击事件
 */
export class T2Button extends HTMLElement {}
customElements.define('x-t2-button', T2Button);
"#;
    let declarations = analyze_source(src);
    assert_eq!(declarations.len(), 1);
    let declaration = &declarations[0];
    assert_eq!(declaration.attributes.len(), 2);
    assert_eq!(declaration.attributes[0].name, "variant");
    assert_eq!(
        declaration.attributes[0].type_text.as_deref(),
        Some("string")
    );
    assert!(declaration.attributes[0].default.is_none());
    assert_eq!(
        declaration.attributes[1].type_text.as_deref(),
        Some("boolean")
    );
    assert_eq!(declaration.slots[0].name, "");
    assert_eq!(declaration.slots[0].description, "按钮文字");
    assert_eq!(declaration.css_properties[0].name, "--x-button-height");
    assert_eq!(declaration.css_parts[0].name, "base");
    assert_eq!(declaration.events[0].name, "x-button-click");
    assert_eq!(declaration.events[0].description, "点击事件");
    assert!(declaration.events[0].type_text.is_none());

    let json = manifest_for_source("t2.ts", src)
        .expect("应有声明")
        .to_json();
    let attributes_section =
        &json[json.find("\"attributes\": [").unwrap()..json.find("\"events\": [").unwrap()];
    assert!(
        !attributes_section.contains("\"default\""),
        "没有默认值时不应输出 default 字段"
    );
    let events_section =
        &json[json.find("\"events\": [").unwrap()..json.find("\"slots\": [").unwrap()];
    assert!(
        !events_section.contains("\"type\""),
        "没有 type 时不应输出空 type 字段"
    );
    assert_json_valid(&json);
}

#[test]
fn fires_supports_braced_type() {
    let src = r#"
/**
 * @fires {CustomEvent<{originalEvent: MouseEvent}>} x-button-click - 说明
 * @fires x-button-close - 关闭
 */
export class Typed extends HTMLElement {}
customElements.define('x-typed', Typed);
"#;
    let declarations = analyze_source(src);
    assert_eq!(declarations.len(), 1);
    let events = &declarations[0].events;
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].name, "x-button-click");
    assert_eq!(
        events[0].type_text.as_deref(),
        Some("CustomEvent<{originalEvent: MouseEvent}>")
    );
    assert_eq!(events[0].description, "说明");
    assert_eq!(events[1].name, "x-button-close");
    assert!(events[1].type_text.is_none());

    let json = manifest_for_source("typed.ts", src)
        .expect("应有声明")
        .to_json();
    let events_section =
        &json[json.find("\"events\": [").unwrap()..json.find("\"slots\": [").unwrap()];
    assert!(events_section.contains("\"text\": \"CustomEvent<{originalEvent: MouseEvent}>\""));
    assert_eq!(events_section.matches("\"type\": {").count(), 1);
    assert_json_valid(&json);
}

// ---------------------------------------------------------------------------
// 跨文件 define 关联（真实源码：class 在 button/x-button.ts，define 在 index.ts）
// ---------------------------------------------------------------------------

#[test]
fn cross_file_define_links_tag_to_class() {
    let button = "export class XButton extends BaseElement {}\n";
    let index = "import './button/x-button.js';\ncustomElements.define('x-button', XButton);\n";

    let (manifest, warnings) =
        analyze_many(&[("src/button/x-button.ts", button), ("src/index.ts", index)]);

    assert!(warnings.is_empty(), "不应报冲突: {warnings:?}");
    assert_eq!(manifest.modules.len(), 1);
    assert_eq!(manifest.modules[0].path, "src/button/x-button.ts");
    let declaration = &manifest.modules[0].declarations[0];
    assert_eq!(declaration.name, "XButton");
    assert_eq!(declaration.tag_name.as_deref(), Some("x-button"));
    assert!(declaration.custom_element);
}

#[test]
fn same_file_define_wins_over_cross_file() {
    let local =
        "export class XButton extends HTMLElement {}\ncustomElements.define('x-local', XButton);\n";
    let remote = "customElements.define('x-remote', XButton);\n";

    let (manifest, warnings) = analyze_many(&[("a.ts", local), ("b.ts", remote)]);

    assert!(warnings.is_empty(), "同文件优先不应报冲突: {warnings:?}");
    assert_eq!(manifest.modules.len(), 1);
    assert_eq!(
        manifest.modules[0].declarations[0].tag_name.as_deref(),
        Some("x-local")
    );
    assert!(manifest.modules[0].declarations[0].custom_element);
}

#[test]
fn cross_file_define_conflict_is_reported_and_skipped() {
    let button = "export class XButton extends HTMLElement {}\n";
    let first = "customElements.define('x-first', XButton);\n";
    let second = "customElements.define('x-second', XButton);\n";

    let (manifest, warnings) = analyze_many(&[
        ("src/button/x-button.ts", button),
        ("src/a.ts", first),
        ("src/b.ts", second),
    ]);

    assert_eq!(manifest.modules.len(), 1);
    let declaration = &manifest.modules[0].declarations[0];
    assert!(declaration.tag_name.is_none(), "冲突时不该随便选一个 tag");
    assert!(!declaration.custom_element);
    assert!(
        warnings
            .iter()
            .any(|warning| warning.contains("define 冲突")),
        "缺少冲突报告: {warnings:?}"
    );
    assert!(warnings.iter().any(|warning| warning.contains("x-first")));
    assert!(warnings.iter().any(|warning| warning.contains("x-second")));
}

#[test]
fn cross_file_define_is_ambiguous_when_class_name_repeats_across_modules() {
    let a = "export class Shared extends HTMLElement {}\n";
    let b = "export class Shared extends HTMLElement {}\n";
    let c = "customElements.define('x-shared', Shared);\n";

    let (manifest, warnings) = analyze_many(&[("a.ts", a), ("b.ts", b), ("c.ts", c)]);

    assert_eq!(manifest.modules.len(), 2);
    for module in &manifest.modules {
        assert!(
            module.declarations[0].tag_name.is_none(),
            "同名类跨文件无法确定归属"
        );
        assert!(!module.declarations[0].custom_element);
    }
    assert!(
        warnings.iter().any(|warning| warning.contains("归属不明")),
        "缺少歧义报告: {warnings:?}"
    );
}

#[test]
fn same_named_classes_keep_their_own_same_file_defines() {
    let a = "export class Shared extends HTMLElement {}\ncustomElements.define('x-a', Shared);\n";
    let b = "export class Shared extends HTMLElement {}\ncustomElements.define('x-b', Shared);\n";

    let (manifest, warnings) = analyze_many(&[("a.ts", a), ("b.ts", b)]);

    assert!(
        warnings.is_empty(),
        "各自有同文件 define 不该报歧义: {warnings:?}"
    );
    assert_eq!(manifest.modules.len(), 2);
    assert_eq!(
        manifest.modules[0].declarations[0].tag_name.as_deref(),
        Some("x-a")
    );
    assert_eq!(
        manifest.modules[1].declarations[0].tag_name.as_deref(),
        Some("x-b")
    );
}

#[test]
fn same_file_multiple_tags_for_one_class_is_reported() {
    let src = "export class XButton extends HTMLElement {}\n\
               customElements.define('x-one', XButton);\n\
               customElements.define('x-two', XButton);\n";
    let (manifest, warnings) = analyze_many(&[("a.ts", src)]);

    assert_eq!(manifest.modules.len(), 1);
    assert!(manifest.modules[0].declarations[0].tag_name.is_none());
    assert!(
        warnings
            .iter()
            .any(|warning| warning.contains("define 冲突")),
        "缺少同文件冲突报告: {warnings:?}"
    );
}

// ---------------------------------------------------------------------------
// 0.1.2 新增：declaration.description 与结构化 default。
// ---------------------------------------------------------------------------

#[test]
fn fixture_declaration_has_first_paragraph_description() {
    let module = fixture_module();
    let declaration = &module.declarations[0];
    assert_eq!(
        declaration.description, "`<x-button>`：按钮组件。",
        "只取 JSDoc 首段，第二个自然段不进 description"
    );

    let json = manifest_for_source(FIXTURE_PATH, FIXTURE)
        .map(|module| {
            let mut manifest = x_cem::emit::Manifest::new(vec![module]);
            manifest.to_json()
        })
        .expect("fixture 应产出模块");
    assert_json_valid(&json);
    assert!(
        json.contains("\"description\": \"`<x-button>`：按钮组件。\""),
        "JSON 里应输出 declaration.description: {json}"
    );
}

#[test]
fn declaration_without_jsdoc_or_summary_omits_description() {
    let src = "export class Bare extends HTMLElement {}\n\
               /**\n\
                * @attr {string} label - 只有标签，没有首段。\n\
                */\n\
               export class TagOnly extends HTMLElement {}\n\
               customElements.define('x-bare', Bare);\n\
               customElements.define('x-tag-only', TagOnly);\n";
    let (manifest, _) = analyze_many(&[("a.ts", src)]);
    let declarations = &manifest.modules[0].declarations;

    assert_eq!(declarations[0].name, "Bare");
    assert_eq!(declarations[0].description, "");
    assert_eq!(declarations[1].name, "TagOnly");
    assert_eq!(declarations[1].description, "");

    let json = manifest_for_source("a.ts", src)
        .map(|module| x_cem::emit::Manifest::new(vec![module]).to_json())
        .expect("应产出模块");
    assert_json_valid(&json);
    assert!(
        !json.contains("\"description\": \"\""),
        "首段为空时不该输出空字符串 description: {json}"
    );
    assert!(
        json.contains("\"description\": \"只有标签，没有首段。\""),
        "标签的 description 不受影响: {json}"
    );
}

#[test]
fn default_supports_assignment_and_text_forms_with_priority() {
    let src = r#"/**
 * `x-probe` 组件。
 *
 * @attr {('sm'|'md'|'lg')} size = md - 显式赋值。
 * @attr {string} label = "two words" - 引号默认值。
 * @attr {number} gap - 文本兜底。Default: 8.
 * @attr {number} ratio - Default: 0.875rem
 * @attr {boolean} disabled - 没有默认值。
 * @attr {('a'|'b')} mode = a - 显式优先。Default: "b".
 * @attr {string} [title='bracket'] - 旧写法继续可用。
 */
export class Probe extends HTMLElement {}
customElements.define('x-probe', Probe);
"#;
    let (manifest, _) = analyze_many(&[("probe.ts", src)]);
    let attributes = &manifest.modules[0].declarations[0].attributes;

    let by_name = |name: &str| {
        attributes
            .iter()
            .find(|attr| attr.name == name)
            .unwrap_or_else(|| panic!("缺少属性 {name}"))
    };

    assert_eq!(by_name("size").default.as_deref(), Some("md"));
    assert_eq!(by_name("label").default.as_deref(), Some("two words"));
    assert_eq!(by_name("gap").default.as_deref(), Some("8"));
    assert_eq!(
        by_name("ratio").default.as_deref(),
        Some("0.875rem"),
        "值内部的点不能被当成句末标点"
    );
    assert_eq!(
        by_name("disabled").default, None,
        "没有默认值的布尔属性不许被填成 false"
    );
    assert_eq!(
        by_name("mode").default.as_deref(),
        Some("a"),
        "显式 JSDoc 默认值优先于描述文本里的 Default:"
    );
    assert_eq!(by_name("title").default.as_deref(), Some("bracket"));
}

// ---------------------------------------------------------------------------
// 测试内置的迷你 JSON 校验器：不引 serde，也能确认输出是真的合法 JSON。
// ---------------------------------------------------------------------------

fn assert_json_valid(text: &str) {
    let mut parser = JsonParser::new(text);
    parser.value();
    parser.skip_ws();
    assert_eq!(parser.index, parser.bytes.len(), "JSON 尾部有多余内容");
}

struct JsonParser<'a> {
    bytes: &'a [u8],
    index: usize,
}

impl<'a> JsonParser<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            bytes: text.as_bytes(),
            index: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.index).copied()
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.index += 1;
        }
    }

    fn expect(&mut self, expected: u8) {
        self.skip_ws();
        assert_eq!(
            self.peek(),
            Some(expected),
            "偏移 {} 期望 `{}`",
            self.index,
            expected as char
        );
        self.index += 1;
    }

    fn value(&mut self) {
        self.skip_ws();
        match self
            .peek()
            .unwrap_or_else(|| panic!("偏移 {} JSON 意外结束", self.index))
        {
            b'{' => self.object(),
            b'[' => self.array(),
            b'"' => self.string(),
            b't' => self.literal("true"),
            b'f' => self.literal("false"),
            b'n' => self.literal("null"),
            b'-' | b'0'..=b'9' => self.number(),
            other => panic!("偏移 {} 非法字符 `{}`", self.index, other as char),
        }
    }

    fn object(&mut self) {
        self.expect(b'{');
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.index += 1;
            return;
        }
        loop {
            self.skip_ws();
            self.string();
            self.expect(b':');
            self.value();
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.index += 1,
                Some(b'}') => {
                    self.index += 1;
                    return;
                }
                _ => panic!("偏移 {} 对象缺少 `,` 或 `}}`", self.index),
            }
        }
    }

    fn array(&mut self) {
        self.expect(b'[');
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.index += 1;
            return;
        }
        loop {
            self.value();
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.index += 1,
                Some(b']') => {
                    self.index += 1;
                    return;
                }
                _ => panic!("偏移 {} 数组缺少 `,` 或 `]`", self.index),
            }
        }
    }

    fn string(&mut self) {
        self.skip_ws();
        self.expect(b'"');
        loop {
            match self.peek() {
                Some(b'"') => {
                    self.index += 1;
                    return;
                }
                Some(b'\\') => {
                    self.index += 1;
                    match self.peek() {
                        Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => {
                            self.index += 1
                        }
                        Some(b'u') => {
                            self.index += 1;
                            for _ in 0..4 {
                                let c = self.peek().expect("\\u 转义不完整");
                                assert!(c.is_ascii_hexdigit(), "\\u 转义含非十六进制字符");
                                self.index += 1;
                            }
                        }
                        other => panic!("非法转义 {:?}", other),
                    }
                }
                Some(c) if c < 0x20 => panic!("字符串里有未转义控制字符"),
                Some(_) => self.index += 1,
                None => panic!("字符串未闭合"),
            }
        }
    }

    fn literal(&mut self, expected: &str) {
        assert!(
            self.bytes[self.index..].starts_with(expected.as_bytes()),
            "期望字面量 {expected}"
        );
        self.index += expected.len();
    }

    fn number(&mut self) {
        if self.peek() == Some(b'-') {
            self.index += 1;
        }
        assert!(matches!(self.peek(), Some(b'0'..=b'9')), "数字缺少整数部分");
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.index += 1;
        }
        if self.peek() == Some(b'.') {
            self.index += 1;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.index += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.index += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.index += 1;
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.index += 1;
            }
        }
    }
}
