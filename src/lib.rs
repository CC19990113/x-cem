//! x-cem：词法级 TS/JS Custom Elements Manifest 生成器。
//!
//! 流程：`lex` 出最小 token 流 → 逐文件收集 class / `customElements.define`
//! （阶段一，不做关联）→ 全局回查 define 映射填 tagName（阶段二）→ 手写序列化。
//! 全程不建 AST、不做类型解析、不做模块解析。
//!
//! 跨文件关联规则：
//! - 同文件 define 优先；同文件出现多个不同 tag 视为冲突，不分配 tagName。
//! - 否则使用跨文件 define；多个不同 tag 视为冲突，不分配 tagName。
//! - 同一个 class 名在多个模块中声明时，跨文件 define 无法确定归属，不分配。
//! - 冲突 / 歧义会以 warning 文本返回，由 CLI 打到 stderr。
//!
//! # 纯核心
//!
//! 本 crate 不碰文件系统、不读环境变量：解析入口只接收 `(path, content)` 字符串。
//! 目录遍历、读写文件、参数解析都在 CLI 层（`src/cli.rs` / `src/main.rs`）；
//! wasm32 侧由 [`wasm_api`] 直接复用同一套纯函数。

pub mod emit;
pub mod jsdoc;
pub mod lexer;

/// wasm32 导出层：手写 `extern "C"`，只在 wasm 目标下编译。
#[cfg(target_arch = "wasm32")]
pub mod wasm_api;

use std::collections::{HashMap, HashSet};

use emit::{Declaration, Manifest, Module};
use lexer::{Kind, Token};

/// 单个文件内识别到的 class 声明。
#[derive(Debug)]
struct RawClass {
    name: String,
    superclass: Option<String>,
    doc: jsdoc::Doc,
}

/// 单个文件内识别到的 `customElements.define(tag, ClassName)`。
#[derive(Debug)]
struct Define {
    tag: String,
    class_name: String,
}

/// 阶段一的产物：只收集事实，不做任何跨文件关联。
#[derive(Debug)]
struct FileAnalysis {
    path: String,
    classes: Vec<RawClass>,
    defines: Vec<Define>,
}

/// 阶段一 + 阶段二一次完成（测试 / 内存中的多文件输入）。
///
/// `files` 的顺序决定 modules 与 declarations 的输出顺序；返回的 warning
/// 是跨文件关联时的冲突或歧义说明。
pub fn analyze_many(files: &[(&str, &str)]) -> (Manifest, Vec<String>) {
    let analyses: Vec<FileAnalysis> = files
        .iter()
        .map(|(path, source)| analyze_one(path, source))
        .collect();
    let (modules, warnings) = assemble(analyses);
    (Manifest::new(modules), warnings)
}

/// 解析单个源码文件（只做同文件关联），返回其中的 custom element 声明。
pub fn analyze_source(src: &str) -> Vec<Declaration> {
    let (manifest, _) = analyze_many(&[("<source>", src)]);
    manifest
        .modules
        .into_iter()
        .next()
        .map(|module| module.declarations)
        .unwrap_or_default()
}

/// 解析单个源码文件，返回该文件的 module 对象（没有声明时 `declarations` 为空）。
///
/// 纯函数：输入路径标签与源码文本，不碰文件系统；wasm 导出层直接复用。
pub fn module_for_source(path: &str, src: &str) -> Module {
    analyze_many(&[(path, src)])
        .0
        .modules
        .into_iter()
        .next()
        .unwrap_or_else(|| Module {
            path: path.to_string(),
            declarations: Vec::new(),
        })
}

/// 解析单个源码文件并装上模块路径；没有任何声明时返回 None。
pub fn manifest_for_source(path: &str, src: &str) -> Option<Module> {
    let module = module_for_source(path, src);
    if module.declarations.is_empty() {
        None
    } else {
        Some(module)
    }
}

/// 词法扫一遍，抽出 class 声明与 define 调用。
fn analyze_one(path: &str, source: &str) -> FileAnalysis {
    let tokens = lexer::lex(source);
    let mut classes: Vec<RawClass> = Vec::new();
    let mut defines: Vec<Define> = Vec::new();
    let mut pending_doc: Option<&str> = None;

    let mut i = 0usize;
    while i < tokens.len() {
        let token = tokens[i];
        match token.kind {
            Kind::JsDoc => pending_doc = Some(token.text),
            Kind::Ident => {
                let word = token.text;
                if word == "class" {
                    if let Some((name, superclass)) = parse_class(&tokens, i) {
                        let doc = pending_doc.take().map(jsdoc::parse).unwrap_or_default();
                        classes.push(RawClass {
                            name,
                            superclass,
                            doc,
                        });
                    }
                    pending_doc = None;
                } else if matches!(word, "export" | "default" | "abstract" | "declare") {
                    // 允许 `/** ... */ export default class X` 这种写法。
                } else {
                    pending_doc = None;
                    if word == "customElements" {
                        if let Some((tag, class_name)) = parse_define(&tokens, i) {
                            defines.push(Define { tag, class_name });
                        }
                    }
                }
            }
            _ => pending_doc = None,
        }
        i += 1;
    }

    FileAnalysis {
        path: path.to_string(),
        classes,
        defines,
    }
}

/// 阶段二：全局回查 define，给 class 填 tagName，组装 modules。
///
/// 先借用 analyses 完成所有 tag 解析，再消费 analyses 移动 JSDoc 数据，
/// 避免为每个 declaration 复制一份 attributes/events/slots。
fn assemble(analyses: Vec<FileAnalysis>) -> (Vec<Module>, Vec<String>) {
    let (tags_per_class, warnings) = {
        let mut defines_by_class: HashMap<&str, Vec<(usize, &str)>> = HashMap::new();
        let mut class_files: HashMap<&str, HashSet<usize>> = HashMap::new();
        for (file_idx, analysis) in analyses.iter().enumerate() {
            for define in &analysis.defines {
                defines_by_class
                    .entry(define.class_name.as_str())
                    .or_default()
                    .push((file_idx, define.tag.as_str()));
            }
            for class in &analysis.classes {
                class_files
                    .entry(class.name.as_str())
                    .or_default()
                    .insert(file_idx);
            }
        }

        let mut warnings = Vec::new();
        let mut rows: Vec<Vec<Option<String>>> = Vec::with_capacity(analyses.len());
        for (file_idx, analysis) in analyses.iter().enumerate() {
            let row = analysis
                .classes
                .iter()
                .map(|class| {
                    resolve_tag(
                        class,
                        file_idx,
                        &analyses,
                        &defines_by_class,
                        &class_files,
                        &mut warnings,
                    )
                })
                .collect();
            rows.push(row);
        }
        (rows, warnings)
    };

    let mut modules = Vec::new();
    for (file_idx, analysis) in analyses.into_iter().enumerate() {
        let FileAnalysis { path, classes, .. } = analysis;
        let mut declarations = Vec::new();
        for (class, tag_name) in classes
            .into_iter()
            .zip(tags_per_class[file_idx].iter().cloned())
        {
            let RawClass {
                name,
                superclass,
                doc,
            } = class;
            let extends_element = superclass
                .as_deref()
                .is_some_and(|sup| sup.contains("Element"));
            if tag_name.is_none() && !extends_element && !doc.has_cem_tags() {
                continue;
            }
            declarations.push(Declaration {
                name,
                description: doc.description,
                custom_element: tag_name.is_some(),
                tag_name,
                attributes: doc.attributes,
                properties: doc.properties,
                events: doc.events,
                slots: doc.slots,
                css_properties: doc.css_properties,
                css_parts: doc.css_parts,
            });
        }
        if !declarations.is_empty() {
            modules.push(Module { path, declarations });
        }
    }
    (modules, warnings)
}

/// 同文件优先；跨文件只在无歧义时使用；冲突写 warning 且不分配。
fn resolve_tag(
    class: &RawClass,
    file_idx: usize,
    analyses: &[FileAnalysis],
    defines_by_class: &HashMap<&str, Vec<(usize, &str)>>,
    class_files: &HashMap<&str, HashSet<usize>>,
    warnings: &mut Vec<String>,
) -> Option<String> {
    let candidates = defines_by_class.get(class.name.as_str())?;

    let same_file: Vec<(usize, &str)> = candidates
        .iter()
        .copied()
        .filter(|(idx, _)| *idx == file_idx)
        .collect();
    if !same_file.is_empty() {
        let tags = distinct_tags(same_file.iter().map(|(_, tag)| *tag));
        if tags.len() == 1 {
            return Some(tags[0].to_string());
        }
        warnings.push(format!(
            "define 冲突：class `{}` 在 {} 内被定义为不同 tag（{}），未分配 tagName",
            class.name,
            analyses[file_idx].path,
            define_detail(&same_file, analyses)
        ));
        return None;
    }

    let tags = distinct_tags(candidates.iter().map(|(_, tag)| *tag));
    if tags.len() > 1 {
        warnings.push(format!(
            "define 冲突：class `{}` 被跨文件 define 为不同 tag（{}），未分配 tagName",
            class.name,
            define_detail(candidates, analyses)
        ));
        return None;
    }

    // 同一个 class 名出现在多个模块里时，纯词法无法判断 define 指向哪一个。
    let owner_files = class_files.get(class.name.as_str())?;
    if owner_files.len() > 1 {
        let mut paths: Vec<&str> = owner_files
            .iter()
            .map(|idx| analyses[*idx].path.as_str())
            .collect();
        paths.sort_unstable();
        warnings.push(format!(
            "跨文件 define 归属不明：class `{}` 同时出现在 {}，未分配 tagName",
            class.name,
            paths.join("、")
        ));
        return None;
    }

    Some(tags[0].to_string())
}

fn distinct_tags<'a>(tags: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    let mut out: Vec<&'a str> = Vec::new();
    for tag in tags {
        if !out.contains(&tag) {
            out.push(tag);
        }
    }
    out
}

fn define_detail(candidates: &[(usize, &str)], analyses: &[FileAnalysis]) -> String {
    candidates
        .iter()
        .map(|(file_idx, tag)| format!("`{tag}` @ {}", analyses[*file_idx].path))
        .collect::<Vec<_>>()
        .join("、")
}

/// 从 `class` token 开始读类名与超类名。
fn parse_class(tokens: &[Token<'_>], class_idx: usize) -> Option<(String, Option<String>)> {
    let name_token = tokens.get(class_idx + 1)?;
    if name_token.kind != Kind::Ident || name_token.text == "extends" {
        // 匿名类（`class extends HTMLElement {}`）暂不支持。
        return None;
    }
    let name = name_token.text.to_string();

    // 允许泛型参数（`class Foo<T> extends Bar`），最多看 64 个 token。
    let mut i = class_idx + 2;
    let mut steps = 0;
    while let Some(token) = tokens.get(i) {
        if steps >= 64 {
            break;
        }
        match token.kind {
            Kind::LBrace => return Some((name, None)),
            Kind::Comma | Kind::RParen | Kind::RBrace => return Some((name, None)),
            Kind::Ident if token.text == "extends" => {
                let superclass = tokens
                    .get(i + 1)
                    .filter(|next| next.kind == Kind::Ident)
                    .map(|next| next.text.to_string());
                return Some((name, superclass));
            }
            _ => {}
        }
        i += 1;
        steps += 1;
    }
    Some((name, None))
}

/// 识别 `customElements.define('tag', ClassName)`；支持多行、引号/反引号、
/// `foo.Bar` 形式的类名。
fn parse_define(tokens: &[Token<'_>], custom_elements_idx: usize) -> Option<(String, String)> {
    if tokens.get(custom_elements_idx + 1)?.kind != Kind::Dot {
        return None;
    }
    let define_token = tokens.get(custom_elements_idx + 2)?;
    if define_token.kind != Kind::Ident || define_token.text != "define" {
        return None;
    }
    if tokens.get(custom_elements_idx + 3)?.kind != Kind::LParen {
        return None;
    }
    let tag_token = tokens.get(custom_elements_idx + 4)?;
    let tag = match tag_token.kind {
        Kind::Str | Kind::Template => unescape_js(tag_token.text),
        _ => return None,
    };
    if !is_valid_tag_name(&tag) {
        return None;
    }
    if tokens.get(custom_elements_idx + 5)?.kind != Kind::Comma {
        return None;
    }

    let mut i = custom_elements_idx + 6;
    let first = tokens.get(i)?;
    if first.kind != Kind::Ident {
        return None;
    }
    let mut class_name = first.text.to_string();
    i += 1;
    while tokens.get(i).is_some_and(|token| token.kind == Kind::Dot) {
        let next = tokens.get(i + 1)?;
        if next.kind != Kind::Ident {
            return None;
        }
        class_name = next.text.to_string();
        i += 2;
    }
    Some((tag, class_name))
}

/// Custom Element 名：小写字母开头、必须含 `-`，其余字符限定在常见集合内。
fn is_valid_tag_name(tag: &str) -> bool {
    let mut chars = tag.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() => {}
        _ => return false,
    }
    tag.contains('-')
        && tag.chars().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '.' | ':')
        })
}

/// 只解码常见转义，够 tagName 用即可。
fn unescape_js(s: &str) -> String {
    if !s.contains('\\') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('\'') => out.push('\''),
            Some('"') => out.push('"'),
            Some('`') => out.push('`'),
            Some('\\') => out.push('\\'),
            Some('u') => {
                let mut hex = String::new();
                if chars.peek() == Some(&'{') {
                    chars.next();
                    while let Some(&next) = chars.peek() {
                        chars.next();
                        if next == '}' {
                            break;
                        }
                        hex.push(next);
                    }
                } else {
                    for _ in 0..4 {
                        if let Some(next) = chars.next() {
                            hex.push(next);
                        }
                    }
                }
                match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    Some(decoded) => out.push(decoded),
                    None => {
                        out.push('u');
                        out.push_str(&hex);
                    }
                }
            }
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}
