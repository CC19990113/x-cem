//! 极简 TS/JS 词法器：只产出 CEM 识别所需的最小 token 流。
//!
//! 必须可靠跳过（任一处失败都会把字符串/注释里的伪造代码当成真代码）：
//!
//! - 行注释 `// ...`
//! - 块注释 `/* ... */`（`/** ... */` 单独保留为 JSDoc token）
//! - 单双引号字符串（含转义）
//! - 模板字符串（含 `${}` 嵌套，嵌套里还可以再有模板字符串）
//! - 正则字面量（含字符类 `[...]` 中的 `/`）
//!
//! 不建 AST、不做类型解析、不读 node_modules。正则与除号的区分用常见的
//! 「前一个 token 是否允许正则」启发式；`if (...) /re/` 这类控制语句括号
//! 用一个小栈单独处理。

/// token 种类：只保留识别三件事（define / class / JSDoc）需要的信息。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Ident,
    /// 引号字符串，text 为去掉两端引号后的原文（不解码转义）。
    Str,
    /// 无插值的模板字符串，text 为反引号之间的原文。
    /// 带 `${}` 的模板不产出 token（tagName 不可能是动态的）。
    Template,
    /// `/** ... */` 原文（含 `/**` 与 `*/`）。
    JsDoc,
    Dot,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Comma,
}

#[derive(Debug, Clone, Copy)]
pub struct Token<'a> {
    pub kind: Kind,
    pub text: &'a str,
}

pub fn lex(src: &str) -> Vec<Token<'_>> {
    let bytes = src.as_bytes();
    let n = bytes.len();
    // 平均每 8 字节一个 token，1000 行文件只有几千个 token。
    let mut out: Vec<Token<'_>> = Vec::with_capacity(n / 8 + 16);
    let mut i = 0usize;
    // 紧跟当前位置的 `/` 是否可能开始一个正则字面量。
    let mut regex_allowed = true;
    // 上一个标识符，用于判断 `(` 前是不是 if/while/for 等控制关键字。
    let mut last_ident: Option<&str> = None;
    // 每个 `(` 是否是控制语句括号；`)` 出栈决定后面能不能接正则。
    let mut paren_stack: Vec<bool> = Vec::new();

    while i < n {
        let b = bytes[i];
        match b {
            b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c => i += 1,
            b'/' => match bytes.get(i + 1).copied() {
                Some(b'/') => i = skip_line_comment(bytes, i + 2),
                Some(b'*') => {
                    // `/**/` 是空块注释；`/** ... */` 才是 JSDoc。
                    let is_doc = bytes.get(i + 2) == Some(&b'*') && bytes.get(i + 3) != Some(&b'/');
                    let end = block_comment_end(bytes, i + 1);
                    if is_doc {
                        out.push(Token {
                            kind: Kind::JsDoc,
                            text: &src[i..end],
                        });
                    }
                    i = end;
                }
                _ if regex_allowed => {
                    let end = skip_regex(bytes, i);
                    if end == i + 1 {
                        // 没构成合法正则（例如行尾孤立的 `/`），按除号处理。
                        regex_allowed = true;
                    } else {
                        regex_allowed = false;
                    }
                    i = end;
                    last_ident = None;
                }
                _ => {
                    i += 1;
                    regex_allowed = true;
                    last_ident = None;
                }
            },
            b'\'' | b'"' => {
                let (end, closed) = skip_string(bytes, i);
                let content_end = if closed { end - 1 } else { end };
                out.push(Token {
                    kind: Kind::Str,
                    text: &src[i + 1..content_end],
                });
                i = end;
                regex_allowed = false;
                last_ident = None;
            }
            b'`' => {
                let (end, has_interp, closed) = skip_template(bytes, i);
                let content_end = if closed { end - 1 } else { end };
                if !has_interp && content_end > i {
                    out.push(Token {
                        kind: Kind::Template,
                        text: &src[i + 1..content_end],
                    });
                }
                i = end;
                regex_allowed = false;
                last_ident = None;
            }
            b'0'..=b'9' => {
                i = skip_number(bytes, i);
                regex_allowed = false;
                last_ident = None;
            }
            b'.' if matches!(bytes.get(i + 1), Some(c) if c.is_ascii_digit()) => {
                i = skip_number(bytes, i);
                regex_allowed = false;
                last_ident = None;
            }
            b'.' => {
                out.push(Token {
                    kind: Kind::Dot,
                    text: ".",
                });
                i += 1;
                regex_allowed = false;
                last_ident = None;
            }
            b'(' => {
                paren_stack.push(matches!(last_ident, Some(w) if is_ctrl_keyword(w)));
                out.push(Token {
                    kind: Kind::LParen,
                    text: "(",
                });
                i += 1;
                regex_allowed = true;
                last_ident = None;
            }
            b')' => {
                let after_ctrl = paren_stack.pop().unwrap_or(false);
                out.push(Token {
                    kind: Kind::RParen,
                    text: ")",
                });
                i += 1;
                regex_allowed = after_ctrl;
                last_ident = None;
            }
            b'{' => {
                out.push(Token {
                    kind: Kind::LBrace,
                    text: "{",
                });
                i += 1;
                regex_allowed = true;
                last_ident = None;
            }
            b'}' => {
                out.push(Token {
                    kind: Kind::RBrace,
                    text: "}",
                });
                i += 1;
                // `}` 之后既可能是块结束也可能是对象字面量结束，按块结束处理（正则优先）。
                regex_allowed = true;
                last_ident = None;
            }
            b',' => {
                out.push(Token {
                    kind: Kind::Comma,
                    text: ",",
                });
                i += 1;
                regex_allowed = true;
                last_ident = None;
            }
            b'+' | b'-' => {
                if bytes.get(i + 1) == Some(&b) {
                    i += 2;
                    regex_allowed = false; // 后缀 ++/-- 之后只能是除号
                } else {
                    i += 1;
                    regex_allowed = true;
                }
                last_ident = None;
            }
            b'[' => {
                i += 1;
                regex_allowed = true;
                last_ident = None;
            }
            b']' => {
                i += 1;
                regex_allowed = false;
                last_ident = None;
            }
            _ if is_ident_start(b) => {
                let end = ident_end(bytes, i);
                let word = &src[i..end];
                out.push(Token {
                    kind: Kind::Ident,
                    text: word,
                });
                regex_allowed = !is_regex_prefix_keyword(word);
                last_ident = Some(word);
                i = end;
            }
            b';' | b':' | b'=' | b'!' | b'&' | b'|' | b'?' | b'*' | b'%' | b'^' | b'~' | b'<'
            | b'>' => {
                i += 1;
                regex_allowed = true;
                last_ident = None;
            }
            _ => {
                i += 1;
                regex_allowed = false;
                last_ident = None;
            }
        }
    }

    out
}

/// 判断某个标识符之后是否允许直接跟正则字面量。
fn is_regex_prefix_keyword(word: &str) -> bool {
    matches!(
        word,
        "return"
            | "typeof"
            | "case"
            | "in"
            | "of"
            | "delete"
            | "void"
            | "throw"
            | "new"
            | "do"
            | "else"
            | "yield"
            | "await"
            | "instanceof"
            | "default"
    )
}

fn is_ctrl_keyword(word: &str) -> bool {
    matches!(word, "if" | "while" | "for" | "switch" | "catch" | "with")
}

fn skip_line_comment(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            return i + 1;
        }
        i += 1;
    }
    i
}

/// `i` 指向块注释的第二个字节（`*`），返回 `*/` 之后的下标。
fn block_comment_end(bytes: &[u8], mut i: usize) -> usize {
    while i + 1 < bytes.len() {
        if bytes[i] == b'*' && bytes[i + 1] == b'/' {
            return i + 2;
        }
        i += 1;
    }
    bytes.len()
}

/// 返回 (结束下标, 是否正常闭合)。未闭合时在换行处截断，避免吞掉整个文件。
fn skip_string(bytes: &[u8], start: usize) -> (usize, bool) {
    let quote = bytes[start];
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i = (i + 2).min(bytes.len()),
            b'\n' => return (i, false),
            c if c == quote => return (i + 1, true),
            _ => i += 1,
        }
    }
    (bytes.len(), false)
}

/// 返回 (结束下标, 是否含 `${}` 插值, 是否正常闭合)。
fn skip_template(bytes: &[u8], start: usize) -> (usize, bool, bool) {
    let mut i = start + 1;
    let mut has_interp = false;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i = (i + 2).min(bytes.len()),
            b'`' => return (i + 1, has_interp, true),
            b'$' if bytes.get(i + 1) == Some(&b'{') => {
                has_interp = true;
                i = skip_braced_code(bytes, i + 2);
            }
            _ => i += 1,
        }
    }
    (bytes.len(), has_interp, false)
}

/// 从 `${` 之后开始，跳过插值表达式直到匹配的 `}`，返回 `}` 之后的下标。
fn skip_braced_code(bytes: &[u8], mut i: usize) -> usize {
    let n = bytes.len();
    let mut depth = 1usize;
    let mut regex_allowed = true;
    while i < n {
        match bytes[i] {
            b'\'' | b'"' => {
                i = skip_string(bytes, i).0;
                regex_allowed = false;
            }
            b'`' => {
                i = skip_template(bytes, i).0;
                regex_allowed = false;
            }
            b'/' => match bytes.get(i + 1).copied() {
                Some(b'/') => i = skip_line_comment(bytes, i + 2),
                Some(b'*') => i = block_comment_end(bytes, i + 1),
                _ if regex_allowed => {
                    let end = skip_regex(bytes, i);
                    i = if end == i + 1 { i + 1 } else { end };
                    regex_allowed = false;
                }
                _ => {
                    i += 1;
                    regex_allowed = true;
                }
            },
            b'{' => {
                depth += 1;
                i += 1;
                regex_allowed = true;
            }
            b'}' => {
                depth -= 1;
                i += 1;
                if depth == 0 {
                    return i;
                }
                regex_allowed = true;
            }
            b')' | b']' => {
                i += 1;
                regex_allowed = false;
            }
            b'(' => {
                i += 1;
                regex_allowed = true;
            }
            b',' | b';' | b':' | b'=' | b'!' | b'&' | b'|' | b'?' | b'+' | b'-' | b'*' | b'%'
            | b'^' | b'~' | b'<' | b'>' => {
                i += 1;
                regex_allowed = true;
            }
            _ if is_ident_start(bytes[i]) => {
                let end = ident_end(bytes, i);
                let word = std::str::from_utf8(&bytes[i..end]).unwrap_or("");
                regex_allowed = is_regex_prefix_keyword(word);
                i = end;
            }
            _ if bytes[i].is_ascii_digit() => {
                i = skip_number(bytes, i);
                regex_allowed = false;
            }
            _ => {
                i += 1;
                regex_allowed = false;
            }
        }
    }
    i
}

/// 返回结束下标。遇到换行视为不是正则，退回只吃一个字符（按除号处理）。
fn skip_regex(bytes: &[u8], start: usize) -> usize {
    let n = bytes.len();
    let mut i = start + 1;
    let mut in_class = false;
    while i < n {
        match bytes[i] {
            b'\\' => i += 2,
            b'\n' => return start + 1,
            b'[' => {
                in_class = true;
                i += 1;
            }
            b']' => {
                in_class = false;
                i += 1;
            }
            b'/' if !in_class => {
                i += 1;
                while i < n && bytes[i].is_ascii_alphabetic() {
                    i += 1;
                }
                return i;
            }
            _ => i += 1,
        }
    }
    n
}

fn skip_number(bytes: &[u8], start: usize) -> usize {
    let n = bytes.len();
    let mut i = start;
    if i + 1 < n && bytes[i] == b'0' {
        match bytes[i + 1] {
            b'x' | b'X' | b'o' | b'O' | b'b' | b'B' => {
                i += 2;
                while i < n && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                return i;
            }
            _ => {}
        }
    }
    while i < n {
        match bytes[i] {
            b'0'..=b'9' | b'_' => i += 1,
            b'.' if matches!(bytes.get(i + 1), Some(c) if c.is_ascii_digit()) => i += 1,
            b'e' | b'E' => {
                i += 1;
                if i < n && (bytes[i] == b'+' || bytes[i] == b'-') {
                    i += 1;
                }
            }
            _ => break,
        }
    }
    i
}

fn ident_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start;
    while i < bytes.len() && is_ident_continue(bytes[i]) {
        i += 1;
    }
    i
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b == b'$' || b >= 0x80
}

fn is_ident_continue(b: u8) -> bool {
    is_ident_start(b) || b.is_ascii_digit()
}
