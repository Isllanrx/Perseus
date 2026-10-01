#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    Script,
    Tsx,
    Css,
    Html,
    Toml,
    Yaml,
    PowerShell,
}

impl Language {
    pub fn of(path: &str) -> Option<Self> {
        let extension = path.rsplit_once('.')?.1.to_ascii_lowercase();
        match extension.as_str() {
            "rs" => Some(Self::Rust),
            "ts" | "mts" | "cts" | "js" | "mjs" | "cjs" => Some(Self::Script),
            "tsx" | "jsx" => Some(Self::Tsx),
            "css" => Some(Self::Css),
            "html" | "htm" => Some(Self::Html),
            "toml" => Some(Self::Toml),
            "yml" | "yaml" => Some(Self::Yaml),
            "ps1" | "psm1" | "psd1" => Some(Self::PowerShell),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Comment {
    pub start: usize,
    pub end: usize,
}

pub fn lex(language: Language, src: &str) -> Result<Vec<Comment>, String> {
    let mut out = Vec::new();
    match language {
        Language::Rust => rust(src, &mut out)?,
        Language::Script | Language::Tsx => {
            Script::at(src, shebang_end(src), &mut out, language == Language::Tsx)
                .code(src.len(), false)?;
        }
        Language::Css => css(src, 0, src.len(), &mut out)?,
        Language::Html => html(src, &mut out)?,
        Language::Toml => toml(src, &mut out)?,
        Language::Yaml => yaml(src, &mut out)?,
        Language::PowerShell => shell_comments(src, &lines(src), Shell::Pwsh, &mut out)?,
    }
    out.sort_by_key(|c| c.start);
    Ok(out)
}

pub fn is_directive(language: Language, src: &str, comment: Comment) -> bool {
    let text = src[comment.start..comment.end].trim();
    if text.contains('\n') {
        return false;
    }
    match language {
        Language::Script | Language::Tsx => {
            text.starts_with("// @ts-")
                || text.starts_with("/// <reference")
                || text.starts_with("// eslint-")
                || text.starts_with("/* eslint-")
        }
        Language::Yaml => {
            text.starts_with("# zizmor:")
                || text.starts_with("# yaml-language-server:")
                || text.starts_with("# shellcheck ")
                || follows_pinned_action(src, comment)
        }
        Language::Toml => text.starts_with("#:schema"),
        Language::PowerShell => text
            .get(..9)
            .is_some_and(|head| head.eq_ignore_ascii_case("#requires")),
        Language::Rust | Language::Css | Language::Html => false,
    }
}

pub fn line_of(src: &str, pos: usize) -> usize {
    src.as_bytes()[..pos.min(src.len())]
        .split(|&b| b == b'\n')
        .count()
}

fn follows_pinned_action(src: &str, comment: Comment) -> bool {
    let line_start = src[..comment.start].rfind('\n').map_or(0, |p| p + 1);
    let code = src[line_start..comment.start].trim();
    let code = code.strip_prefix("- ").map_or(code, str::trim_start);
    code.starts_with("uses:") && code.contains('@')
}

fn shebang_end(src: &str) -> usize {
    if src.starts_with("#!") {
        line_end(src.as_bytes(), 0)
    } else {
        0
    }
}

fn unterminated(src: &str, pos: usize, what: &str) -> String {
    format!("line {}: unterminated {what}", line_of(src, pos))
}

fn line_end(b: &[u8], from: usize) -> usize {
    let mut j = from;
    while j < b.len() && b[j] != b'\n' {
        j += 1;
    }
    if j > from && b[j - 1] == b'\r' {
        j - 1
    } else {
        j
    }
}

fn find(b: &[u8], from: usize, to: usize, needle: &[u8]) -> Option<usize> {
    let last = to.checked_sub(needle.len())?;
    (from..=last).find(|&j| &b[j..j + needle.len()] == needle)
}

fn find_ignore_case(b: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    let last = b.len().checked_sub(needle.len())?;
    (from..=last).find(|&j| b[j..j + needle.len()].eq_ignore_ascii_case(needle))
}

fn quoted_end(b: &[u8], open: usize, to: usize, quote: u8, escapes: bool) -> Option<usize> {
    let mut j = open + 1;
    while j < to {
        if escapes && b[j] == b'\\' {
            j += 2;
            continue;
        }
        if b[j] == quote {
            return Some(j + 1);
        }
        j += 1;
    }
    None
}

fn is_word_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c >= 0x80
}

fn word_end(b: &[u8], from: usize, dollar: bool) -> usize {
    let mut j = from;
    while j < b.len() && (is_word_byte(b[j]) || (dollar && b[j] == b'$')) {
        j += 1;
    }
    j
}

fn rust(src: &str, out: &mut Vec<Comment>) -> Result<(), String> {
    let b = src.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let next = b.get(i + 1).copied();
        match b[i] {
            b'/' if next == Some(b'/') => {
                let end = line_end(b, i);
                out.push(Comment { start: i, end });
                i = end;
            }
            b'/' if next == Some(b'*') => {
                let end =
                    rust_block_end(b, i).ok_or_else(|| unterminated(src, i, "block comment"))?;
                out.push(Comment { start: i, end });
                i = end;
            }
            b'"' => {
                i = quoted_end(b, i, b.len(), b'"', true)
                    .ok_or_else(|| unterminated(src, i, "string"))?;
            }
            b'\'' => i = rust_quote(src, i)?,
            c if c.is_ascii_digit() => i = word_end(b, i, false),
            c if is_word_byte(c) => i = rust_word(src, i)?,
            _ => i += 1,
        }
    }
    Ok(())
}

fn rust_block_end(b: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut j = open;
    while j + 1 < b.len() {
        if b[j] == b'/' && b[j + 1] == b'*' {
            depth += 1;
            j += 2;
        } else if b[j] == b'*' && b[j + 1] == b'/' {
            depth -= 1;
            j += 2;
            if depth == 0 {
                return Some(j);
            }
        } else {
            j += 1;
        }
    }
    None
}

fn rust_quote(src: &str, open: usize) -> Result<usize, String> {
    let b = src.as_bytes();
    let Some(first) = src[open + 1..].chars().next() else {
        return Ok(open + 1);
    };
    if first == '\\' {
        let escaped = open + 2;
        let skip = src[escaped..].chars().next().map_or(0, char::len_utf8);
        let mut j = escaped + skip;
        while j < b.len() && b[j] != b'\'' {
            if b[j] == b'\n' {
                return Err(unterminated(src, open, "character literal"));
            }
            j += 1;
        }
        if j == b.len() {
            return Err(unterminated(src, open, "character literal"));
        }
        return Ok(j + 1);
    }
    let after = open + 1 + first.len_utf8();
    if b.get(after) == Some(&b'\'') {
        Ok(after + 1)
    } else {
        Ok(open + 1)
    }
}

fn rust_word(src: &str, start: usize) -> Result<usize, String> {
    let b = src.as_bytes();
    let end = word_end(b, start, false);
    match &src[start..end] {
        "r" | "br" | "cr" => {
            let mut k = end;
            while b.get(k) == Some(&b'#') {
                k += 1;
            }
            if b.get(k) != Some(&b'"') {
                return Ok(end);
            }
            let hashes = k - end;
            let mut j = k + 1;
            while j < b.len() {
                if b[j] == b'"'
                    && b.get(j + 1..j + 1 + hashes)
                        .is_some_and(|h| h.iter().all(|&c| c == b'#'))
                {
                    return Ok(j + 1 + hashes);
                }
                j += 1;
            }
            Err(unterminated(src, start, "raw string"))
        }
        "b" | "c" if b.get(end) == Some(&b'"') => quoted_end(b, end, b.len(), b'"', true)
            .ok_or_else(|| unterminated(src, start, "string")),
        "b" if b.get(end) == Some(&b'\'') => rust_quote(src, end),
        _ => Ok(end),
    }
}

const REGEX_AFTER: [&str; 15] = [
    "return",
    "typeof",
    "instanceof",
    "in",
    "of",
    "new",
    "delete",
    "void",
    "throw",
    "case",
    "do",
    "else",
    "yield",
    "await",
    "extends",
];

struct Script<'a> {
    src: &'a str,
    b: &'a [u8],
    i: usize,
    out: &'a mut Vec<Comment>,
    jsx: bool,
}

impl<'a> Script<'a> {
    fn at(src: &'a str, from: usize, out: &'a mut Vec<Comment>, jsx: bool) -> Self {
        Self {
            src,
            b: src.as_bytes(),
            i: from,
            out,
            jsx,
        }
    }

    fn code(&mut self, to: usize, until_brace: bool) -> Result<(), String> {
        let mut depth = 0usize;
        let mut regex_allowed = true;
        while self.i < to {
            let c = self.b[self.i];
            let next = if self.i + 1 < to {
                Some(self.b[self.i + 1])
            } else {
                None
            };
            match c {
                b'/' if next == Some(b'/') => {
                    let end = line_end(self.b, self.i).min(to);
                    self.out.push(Comment { start: self.i, end });
                    self.i = end;
                }
                b'/' if next == Some(b'*') => {
                    let close = find(self.b, self.i + 2, to, b"*/")
                        .ok_or_else(|| unterminated(self.src, self.i, "block comment"))?;
                    self.out.push(Comment {
                        start: self.i,
                        end: close + 2,
                    });
                    self.i = close + 2;
                }
                b'<' if self.jsx
                    && regex_allowed
                    && next.is_some_and(|n| n.is_ascii_alphabetic() || n == b'>') =>
                {
                    if self.generic_parameters(to) {
                        regex_allowed = true;
                    } else {
                        self.jsx_element(to)?;
                        regex_allowed = false;
                    }
                }
                b'/' if regex_allowed => {
                    self.i = self.regex_end(to)?;
                    regex_allowed = false;
                }
                b'"' | b'\'' => {
                    self.i = quoted_end(self.b, self.i, to, c, true)
                        .ok_or_else(|| unterminated(self.src, self.i, "string"))?;
                    regex_allowed = false;
                }
                b'`' => {
                    self.template(to)?;
                    regex_allowed = false;
                }
                b'{' => {
                    depth += 1;
                    self.i += 1;
                    regex_allowed = true;
                }
                b'}' if until_brace && depth == 0 => {
                    self.i += 1;
                    return Ok(());
                }
                b'}' => {
                    depth = depth.saturating_sub(1);
                    self.i += 1;
                    regex_allowed = false;
                }
                b')' | b']' => {
                    self.i += 1;
                    regex_allowed = false;
                }
                b'+' | b'-' if next == Some(c) => {
                    self.i += 2;
                    regex_allowed = false;
                }
                c if is_word_byte(c) || c == b'$' => {
                    let end = word_end(self.b, self.i, true);
                    regex_allowed = REGEX_AFTER.contains(&&self.src[self.i..end]);
                    self.i = end;
                }
                c if c.is_ascii_whitespace() => self.i += 1,
                _ => {
                    self.i += 1;
                    regex_allowed = true;
                }
            }
        }
        if until_brace {
            return Err(unterminated(self.src, self.i, "expression in braces"));
        }
        Ok(())
    }

    fn generic_parameters(&mut self, to: usize) -> bool {
        let name_end = word_end(self.b, self.i + 1, true);
        let mut j = name_end;
        while j < to && self.b[j].is_ascii_whitespace() {
            j += 1;
        }
        let generic = name_end > self.i + 1
            && (self.b.get(j) == Some(&b',') || self.src[j..to].starts_with("extends "));
        if !generic {
            return false;
        }
        let mut depth = 0usize;
        while self.i < to {
            match self.b[self.i] {
                b'<' => depth += 1,
                b'>' if self.b[self.i - 1] != b'=' => {
                    depth -= 1;
                    if depth == 0 {
                        self.i += 1;
                        return true;
                    }
                }
                _ => {}
            }
            self.i += 1;
        }
        true
    }

    fn jsx_element(&mut self, to: usize) -> Result<(), String> {
        let open = self.i;
        self.i += 1;
        if self.b.get(self.i) == Some(&b'>') {
            self.i += 1;
            return self.jsx_children(to, open);
        }
        while self.i < to
            && (is_word_byte(self.b[self.i]) || matches!(self.b[self.i], b'.' | b':' | b'-' | b'$'))
        {
            self.i += 1;
        }
        while self.i < to {
            let next = self.b.get(self.i + 1).copied();
            match self.b[self.i] {
                b'/' if next == Some(b'>') => {
                    self.i += 2;
                    return Ok(());
                }
                b'>' => {
                    self.i += 1;
                    return self.jsx_children(to, open);
                }
                b'{' => {
                    self.i += 1;
                    self.code(to, true)?;
                }
                b'/' if next == Some(b'/') => {
                    let end = line_end(self.b, self.i).min(to);
                    self.out.push(Comment { start: self.i, end });
                    self.i = end;
                }
                b'/' if next == Some(b'*') => {
                    let close = find(self.b, self.i + 2, to, b"*/")
                        .ok_or_else(|| unterminated(self.src, self.i, "block comment"))?;
                    self.out.push(Comment {
                        start: self.i,
                        end: close + 2,
                    });
                    self.i = close + 2;
                }
                q @ (b'"' | b'\'') => {
                    self.i = quoted_end(self.b, self.i, to, q, false)
                        .ok_or_else(|| unterminated(self.src, self.i, "JSX attribute"))?;
                }
                _ => self.i += 1,
            }
        }
        Err(unterminated(self.src, open, "JSX element"))
    }

    fn jsx_children(&mut self, to: usize, open: usize) -> Result<(), String> {
        while self.i < to {
            match self.b[self.i] {
                b'<' if self.b.get(self.i + 1) == Some(&b'/') => {
                    let close = find(self.b, self.i, to, b">")
                        .ok_or_else(|| unterminated(self.src, self.i, "JSX closing tag"))?;
                    self.i = close + 1;
                    return Ok(());
                }
                b'<' => self.jsx_element(to)?,
                b'{' => self.jsx_expression(to)?,
                _ => self.i += 1,
            }
        }
        Err(unterminated(self.src, open, "JSX element"))
    }

    fn jsx_expression(&mut self, to: usize) -> Result<(), String> {
        let open = self.i;
        let first = self.out.len();
        self.i += 1;
        self.code(to, true)?;
        let close = self.i - 1;
        let mut j = open + 1;
        let mut only_comments = self.out.len() > first;
        for comment in &self.out[first..] {
            only_comments &= self.src[j..comment.start].trim().is_empty();
            j = comment.end;
        }
        only_comments &= self.src[j..close].trim().is_empty();
        if only_comments {
            self.out.truncate(first);
            self.out.push(Comment {
                start: open,
                end: close + 1,
            });
        }
        Ok(())
    }

    fn regex_end(&self, to: usize) -> Result<usize, String> {
        let mut j = self.i + 1;
        let mut in_class = false;
        while j < to {
            match self.b[j] {
                b'\\' => j += 1,
                b'[' => in_class = true,
                b']' => in_class = false,
                b'/' if !in_class => return Ok(word_end(self.b, j + 1, false)),
                b'\n' => break,
                _ => {}
            }
            j += 1;
        }
        Err(unterminated(self.src, self.i, "regular expression"))
    }

    fn template(&mut self, to: usize) -> Result<(), String> {
        let open = self.i;
        self.i += 1;
        while self.i < to {
            match self.b[self.i] {
                b'\\' => self.i += 2,
                b'`' => {
                    self.i += 1;
                    return Ok(());
                }
                b'$' if self.b.get(self.i + 1) == Some(&b'{') => {
                    self.i += 2;
                    self.code(to, true)?;
                }
                _ => self.i += 1,
            }
        }
        Err(unterminated(self.src, open, "template literal"))
    }
}

fn css(src: &str, from: usize, to: usize, out: &mut Vec<Comment>) -> Result<(), String> {
    let b = src.as_bytes();
    let mut i = from;
    while i < to {
        match b[i] {
            b'/' if i + 1 < to && b[i + 1] == b'*' => {
                let close =
                    find(b, i + 2, to, b"*/").ok_or_else(|| unterminated(src, i, "CSS comment"))?;
                out.push(Comment {
                    start: i,
                    end: close + 2,
                });
                i = close + 2;
            }
            q @ (b'"' | b'\'') => {
                i = quoted_end(b, i, to, q, true)
                    .ok_or_else(|| unterminated(src, i, "CSS string"))?;
            }
            _ => i += 1,
        }
    }
    Ok(())
}

fn html(src: &str, out: &mut Vec<Comment>) -> Result<(), String> {
    let b = src.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i..].starts_with(b"<!--") {
            let close = find(b, i + 4, b.len(), b"-->")
                .ok_or_else(|| unterminated(src, i, "HTML comment"))?;
            out.push(Comment {
                start: i,
                end: close + 3,
            });
            i = close + 3;
            continue;
        }
        if b[i] != b'<' || !b.get(i + 1).is_some_and(u8::is_ascii_alphabetic) {
            i += 1;
            continue;
        }
        let name_end = word_end(b, i + 1, false);
        let name = src[i + 1..name_end].to_ascii_lowercase();
        let mut j = name_end;
        while j < b.len() && b[j] != b'>' {
            if b[j] == b'"' || b[j] == b'\'' {
                j = quoted_end(b, j, b.len(), b[j], false)
                    .ok_or_else(|| unterminated(src, j, "attribute value"))?;
            } else {
                j += 1;
            }
        }
        let content = (j + 1).min(b.len());
        let closing: &[u8] = match name.as_str() {
            "script" => b"</script",
            "style" => b"</style",
            _ => {
                i = content;
                continue;
            }
        };
        let content_end =
            find_ignore_case(b, content, closing).ok_or_else(|| unterminated(src, i, &name))?;
        if name == "script" {
            Script::at(src, content, out, false).code(content_end, false)?;
        } else {
            css(src, content, content_end, out)?;
        }
        i = content_end;
    }
    Ok(())
}

fn toml(src: &str, out: &mut Vec<Comment>) -> Result<(), String> {
    let b = src.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'#' => {
                let end = line_end(b, i);
                out.push(Comment { start: i, end });
                i = end;
            }
            b'"' if b[i..].starts_with(b"\"\"\"") => {
                let mut j = i + 3;
                loop {
                    if j + 3 > b.len() {
                        return Err(unterminated(src, i, "multi-line string"));
                    }
                    if b[j] == b'\\' {
                        j += 2;
                    } else if b[j..].starts_with(b"\"\"\"") {
                        break;
                    } else {
                        j += 1;
                    }
                }
                i = j + 3;
            }
            b'\'' if b[i..].starts_with(b"'''") => {
                let close = find(b, i + 3, b.len(), b"'''")
                    .ok_or_else(|| unterminated(src, i, "multi-line literal string"))?;
                i = close + 3;
            }
            q @ (b'"' | b'\'') => {
                i = quoted_end(b, i, b.len(), q, q == b'"')
                    .ok_or_else(|| unterminated(src, i, "string"))?;
            }
            _ => i += 1,
        }
    }
    Ok(())
}

fn lines(src: &str) -> Vec<(usize, usize)> {
    let b = src.as_bytes();
    let mut spans = Vec::new();
    let mut start = 0;
    while start < b.len() {
        let end = line_end(b, start);
        spans.push((start, end));
        let mut next = end;
        while next < b.len() && b[next] != b'\n' {
            next += 1;
        }
        start = next + 1;
    }
    spans
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start_matches([' ', '\t']).len()
}

struct YamlLine {
    key: Option<String>,
    key_column: usize,
    block_scalar: bool,
    value: String,
}

fn yaml(src: &str, out: &mut Vec<Comment>) -> Result<(), String> {
    let spans = lines(src);
    let mut open_quote: Option<u8> = None;
    let mut windows_runner = false;
    let mut idx = 0;
    while idx < spans.len() {
        let (start, end) = spans[idx];
        let line = yaml_line(src, start, end, &mut open_quote, out);
        if line.key.as_deref() == Some("runs-on") {
            windows_runner = line.value.contains("windows");
        }
        if !line.block_scalar {
            idx += 1;
            continue;
        }
        let parent = indent(&src[start..end]);
        let mut content_indent = None;
        let mut k = idx + 1;
        while k < spans.len() {
            let text = &src[spans[k].0..spans[k].1];
            if text.trim().is_empty() {
                k += 1;
                continue;
            }
            let this = indent(text);
            match content_indent {
                None if this > parent => content_indent = Some(this),
                None => break,
                Some(ci) if this < ci => break,
                Some(_) => {}
            }
            k += 1;
        }
        if line.key.as_deref() == Some("run") && k > idx + 1 {
            let shell =
                step_shell(src, &spans, idx, k, line.key_column).unwrap_or(if windows_runner {
                    Shell::Pwsh
                } else {
                    Shell::Bash
                });
            shell_comments(src, &spans[idx + 1..k], shell, out)?;
        }
        idx = k;
    }
    if open_quote.is_some() {
        return Err("unterminated quoted scalar at end of file".into());
    }
    Ok(())
}

fn yaml_line(
    src: &str,
    start: usize,
    end: usize,
    open_quote: &mut Option<u8>,
    out: &mut Vec<Comment>,
) -> YamlLine {
    let b = src.as_bytes();
    let mut i = start;
    let mut result = YamlLine {
        key: None,
        key_column: 0,
        block_scalar: false,
        value: String::new(),
    };
    if let Some(q) = *open_quote {
        match yaml_quote_end(b, i, end, q) {
            Some(j) => {
                *open_quote = None;
                i = j;
            }
            None => return result,
        }
    }
    let mut value_start_ok = true;
    let mut content_start: Option<usize> = None;
    let mut value_from: Option<usize> = None;
    let mut code_end = end;
    let mut flow = 0usize;
    while i < end {
        let c = b[i];
        match c {
            b' ' | b'\t' => i += 1,
            b'#' if i == start || b[i - 1] == b' ' || b[i - 1] == b'\t' => {
                out.push(Comment { start: i, end });
                code_end = i;
                break;
            }
            b'\'' | b'"' if value_start_ok => {
                let Some(j) = yaml_quote_end(b, i + 1, end, c) else {
                    *open_quote = Some(c);
                    return result;
                };
                content_start.get_or_insert(i);
                i = j;
                value_start_ok = false;
            }
            b'-' if value_start_ok
                && content_start.is_none()
                && (i + 1 == end || b[i + 1] == b' ') =>
            {
                i += 1;
            }
            b':' if flow == 0
                && result.key.is_none()
                && (i + 1 == end || b[i + 1] == b' ' || b[i + 1] == b'\t') =>
            {
                if let Some(key_start) = content_start {
                    result.key = Some(src[key_start..i].trim().to_string());
                    result.key_column = key_start - start;
                }
                i += 1;
                value_start_ok = true;
                value_from = Some(i);
            }
            b'[' | b'{' if value_start_ok => {
                flow += 1;
                content_start.get_or_insert(i);
                i += 1;
            }
            b']' | b'}' if flow > 0 => {
                flow -= 1;
                i += 1;
                value_start_ok = false;
            }
            b',' if flow > 0 => {
                i += 1;
                value_start_ok = true;
            }
            _ => {
                content_start.get_or_insert(i);
                value_start_ok = false;
                i += 1;
            }
        }
    }
    if let Some(from) = value_from {
        let value = src[from..code_end.max(from)].trim();
        result.value = value.to_string();
        let mut chars = value.chars();
        result.block_scalar = matches!(chars.next(), Some('|' | '>'))
            && chars.all(|ch| ch == '+' || ch == '-' || ch.is_ascii_digit());
    }
    result
}

fn yaml_quote_end(b: &[u8], from: usize, end: usize, quote: u8) -> Option<usize> {
    let mut j = from;
    while j < end {
        if quote == b'"' && b[j] == b'\\' {
            j += 2;
            continue;
        }
        if b[j] == quote {
            if quote == b'\'' && b.get(j + 1) == Some(&b'\'') {
                j += 2;
                continue;
            }
            return Some(j + 1);
        }
        j += 1;
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shell {
    Bash,
    Pwsh,
}

fn key_line(text: &str) -> Option<(usize, &str, &str)> {
    let ind = indent(text);
    let rest = &text[ind..];
    let (column, rest) = match rest.strip_prefix("- ") {
        Some(item) => (ind + 2, item),
        None => (ind, rest),
    };
    let (key, value) = rest.split_once(':')?;
    Some((column, key.trim(), value.trim()))
}

fn step_shell(
    src: &str,
    spans: &[(usize, usize)],
    run_line: usize,
    after_block: usize,
    column: usize,
) -> Option<Shell> {
    let as_shell = |value: &str| {
        let value = value.trim_matches(|c| c == '\'' || c == '"');
        if value.starts_with("pwsh") || value.starts_with("powershell") {
            Shell::Pwsh
        } else {
            Shell::Bash
        }
    };
    let opens_item = src[spans[run_line].0..spans[run_line].1]
        .trim_start()
        .starts_with("- ");
    let before = if opens_item {
        &[][..]
    } else {
        &spans[..run_line]
    };
    for &(s, e) in before.iter().rev() {
        let text = &src[s..e];
        if text.trim().is_empty() {
            continue;
        }
        let ind = indent(text);
        if let Some(value) = shell_value(text, column) {
            return Some(as_shell(value));
        }
        if ind < column {
            break;
        }
    }
    for &(s, e) in &spans[after_block..] {
        let text = &src[s..e];
        if text.trim().is_empty() {
            continue;
        }
        if indent(text) < column {
            break;
        }
        if let Some(value) = shell_value(text, column) {
            return Some(as_shell(value));
        }
    }
    None
}

fn shell_value(text: &str, column: usize) -> Option<&str> {
    let (col, key, value) = key_line(text)?;
    (col == column && key == "shell").then_some(value)
}

#[allow(
    clippy::too_many_lines,
    reason = "maquina de estados de um lexer: dividir espalharia o estado entre linhas"
)]
fn shell_comments(
    src: &str,
    spans: &[(usize, usize)],
    shell: Shell,
    out: &mut Vec<Comment>,
) -> Result<(), String> {
    let b = src.as_bytes();
    let mut heredoc: Option<String> = None;
    let mut here_string: Option<u8> = None;
    let mut block_comment: Option<usize> = None;
    let mut quote: Option<u8> = None;
    for &(start, end) in spans {
        let text = &src[start..end];
        if let Some(terminator) = &heredoc {
            if text.trim() == terminator {
                heredoc = None;
            }
            continue;
        }
        if let Some(q) = here_string {
            let t = text.trim_start().as_bytes();
            if t.len() >= 2 && t[0] == q && t[1] == b'@' {
                here_string = None;
            }
            continue;
        }
        let mut i = start;
        if let Some(open) = block_comment {
            match find(b, i, end, b"#>") {
                Some(close) => {
                    out.push(Comment {
                        start: open,
                        end: close + 2,
                    });
                    block_comment = None;
                    i = close + 2;
                }
                None => continue,
            }
        }
        while i < end {
            let c = b[i];
            if let Some(q) = quote {
                let escape = if shell == Shell::Bash { b'\\' } else { b'`' };
                if c == escape && q == b'"' {
                    i += 2;
                    continue;
                }
                if c == q {
                    quote = None;
                }
                i += 1;
                continue;
            }
            let word_start = src[start..i].trim().is_empty()
                || matches!(
                    (shell, b[i - 1]),
                    (_, b' ' | b'\t' | b';' | b'|' | b'(')
                        | (Shell::Bash, b'&')
                        | (Shell::Pwsh, b'{' | b'}')
                );
            match c {
                b'#' if word_start => {
                    out.push(Comment { start: i, end });
                    break;
                }
                b'<' if shell == Shell::Pwsh && word_start && b.get(i + 1) == Some(&b'#') => {
                    let Some(close) = find(b, i + 2, end, b"#>") else {
                        block_comment = Some(i);
                        break;
                    };
                    out.push(Comment {
                        start: i,
                        end: close + 2,
                    });
                    i = close + 2;
                }
                b'\\' if shell == Shell::Bash => i += 2,
                b'`' if shell == Shell::Pwsh => i += 2,
                b'@' if shell == Shell::Pwsh
                    && matches!(b.get(i + 1), Some(b'\'' | b'"'))
                    && src[i + 2..end].trim().is_empty() =>
                {
                    here_string = Some(b[i + 1]);
                    break;
                }
                b'<' if shell == Shell::Bash
                    && b.get(i + 1) == Some(&b'<')
                    && b.get(i + 2) != Some(&b'<')
                    && (i == start || b[i - 1] != b'<') =>
                {
                    let mut j = i + 2;
                    if b.get(j) == Some(&b'-') {
                        j += 1;
                    }
                    while j < end && b[j] == b' ' {
                        j += 1;
                    }
                    let quoted = j < end && (b[j] == b'\'' || b[j] == b'"');
                    let word_from = if quoted { j + 1 } else { j };
                    let word_to = word_end(b, word_from, false);
                    heredoc = Some(src[word_from..word_to].to_string());
                    i = if quoted { word_to + 1 } else { word_to };
                }
                b'\'' | b'"' => {
                    quote = Some(c);
                    i += 1;
                }
                _ => i += 1,
            }
        }
    }
    if heredoc.is_some() || here_string.is_some() || block_comment.is_some() {
        return Err(unterminated(
            src,
            spans.first().map_or(0, |s| s.0),
            "shell block",
        ));
    }
    Ok(())
}
