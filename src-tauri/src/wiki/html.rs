//! 極簡 HTML 抽取工具（不新增依賴）。
//!
//! 只依賴語意結構（table 欄位名、h1–h6 標題文字、格子文字與連結），
//! 不依賴 AtWiki 外層 CSS class，也不依賴第幾個 table/div。

/// 表格格子：保留 inner HTML，文字與連結由呼叫端抽取。
pub struct Cell {
    pub inner: String,
}

pub struct Row {
    pub cells: Vec<Cell>,
}

pub struct Table {
    pub rows: Vec<Row>,
}

/// 標題元素：`tag_start` 是 `<hN` 的位置，`content_end` 是 `</hN>` 結尾之後。
pub struct Heading {
    pub level: u8,
    pub text: String,
    pub tag_start: usize,
    pub content_end: usize,
}

pub struct Link {
    pub href: String,
    pub text: String,
}

/// 移除註解、script 與 style 區塊，避免內容中的 `<` 被當成標籤。
pub fn cleaned_html(html: &str) -> String {
    strip_ignored(html)
}

fn strip_ignored(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut i = 0;
    while i < html.len() {
        if lower[i..].starts_with("<!--") {
            if let Some(end) = lower[i..].find("-->") {
                i += end + 3;
                continue;
            }
            break;
        }
        let script = is_tag_open(&lower, i, "script");
        let style = !script && is_tag_open(&lower, i, "style");
        if script || style {
            let close = if script { "</script" } else { "</style" };
            match lower[i..].find(close) {
                Some(end) => match lower[i + end..].find('>') {
                    Some(gt) => i += end + gt + 1,
                    None => break,
                },
                None => break,
            }
            out.push('\n');
            continue;
        }
        let ch = html[i..].chars().next().unwrap_or('\u{FFFD}');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// 位置 `i` 是否為 `<name` 標籤開頭（下一個字元為空白或 `>`）。
fn is_tag_open(lower: &str, i: usize, name: &str) -> bool {
    let open = format!("<{name}");
    if !lower[i..].starts_with(&open) {
        return false;
    }
    match lower.as_bytes().get(i + open.len()) {
        None => true,
        Some(b) => b.is_ascii_whitespace() || *b == b'>',
    }
}

/// 掃描位置 `pos` 的 `<...>`，回傳（是否為關閉標籤，小寫標籤名，標籤結尾的下一個位置）。
fn parse_tag(s: &str, pos: usize) -> Option<(bool, String, usize)> {
    let bytes = s.as_bytes();
    if bytes.get(pos) != Some(&b'<') {
        return None;
    }
    let mut i = pos + 1;
    let is_close = if bytes.get(i) == Some(&b'/') {
        i += 1;
        true
    } else {
        false
    };
    let name_start = i;
    while let Some(&b) = bytes.get(i) {
        if b.is_ascii_alphanumeric() || b == b'!' || b == b'?' {
            i += 1;
        } else {
            break;
        }
    }
    if i == name_start {
        return None;
    }
    let name = s[name_start..i].to_ascii_lowercase();
    // 跳過屬性（處理引號內的 `>`）。
    let mut quote = None;
    while let Some(&b) = bytes.get(i) {
        if let Some(q) = quote {
            if b == q {
                quote = None;
            }
        } else if b == b'"' || b == b'\'' {
            quote = Some(b);
        } else if b == b'>' {
            i += 1;
            break;
        }
        i += 1;
    }
    Some((is_close, name, i))
}

/// 找出所有 `<table>...</table>`（支援巢狀，以最外層為準）。
pub fn extract_tables(html: &str) -> Vec<Table> {
    let cleaned = strip_ignored(html);
    let s = cleaned.as_str();
    let mut tables = Vec::new();
    let mut pos = 0;
    while let Some(next) = find_open_tag(s, pos, "table") {
        let mut depth = 0;
        let mut cursor = next;
        let mut inner_end = None;
        while cursor < s.len() {
            match parse_tag(s, cursor) {
                Some((is_close, name, end)) => {
                    if name == "table" {
                        if is_close {
                            depth -= 1;
                            if depth == 0 {
                                inner_end = Some((next, cursor));
                                pos = end;
                                break;
                            }
                        } else {
                            depth += 1;
                        }
                    }
                    cursor = end;
                }
                None => {
                    // 非標籤文字：跳到下一個 `<`，避免停在表格文字中間。
                    match s[cursor..].find('<') {
                        Some(0) => cursor += 1,
                        Some(rel) => cursor += rel,
                        None => break,
                    }
                }
            }
        }
        // 內容起點：第一個 `<table...>` 的結尾。
        if let Some((table_open, table_close_start)) = inner_end {
            if let Some((_, _, content_start)) = parse_tag(s, table_open) {
                let inner = &s[content_start..table_close_start];
                tables.push(Table {
                    rows: extract_rows(inner),
                });
            }
        } else {
            pos = next + 1;
        }
    }
    tables
}

/// 在 `from` 之後找 `<name ...>` 開啟標籤（`<namex` 這種前綴不算）。
fn find_open_tag(s: &str, from: usize, name: &str) -> Option<usize> {
    find_open_tag_fast(s, from, name)
}

fn find_open_tag_fast(s: &str, from: usize, name: &str) -> Option<usize> {
    let lower = s.to_ascii_lowercase();
    let mut search = from;
    loop {
        let rel = lower[search..].find(&format!("<{name}"))?;
        let pos = search + rel;
        let after = pos + 1 + name.len();
        match s.as_bytes().get(after) {
            Some(b) if b.is_ascii_alphanumeric() => {
                search = after;
                continue;
            }
            _ => return Some(pos),
        }
    }
}

fn extract_rows(inner: &str) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut search = 0;
    while let Some(open) = find_open_tag_fast(inner, search, "tr") {
        let content_start = match parse_tag(inner, open) {
            Some((_, _, end)) => end,
            None => break,
        };
        let lower = inner.to_ascii_lowercase();
        let close = match lower[content_start..].find("</tr") {
            Some(rel) => content_start + rel,
            None => break,
        };
        let close_end = match parse_tag(inner, close) {
            Some((_, _, end)) => end,
            None => break,
        };
        rows.push(Row {
            cells: extract_cells(&inner[content_start..close]),
        });
        search = close_end;
    }
    rows
}

fn extract_cells(row_inner: &str) -> Vec<Cell> {
    let mut cells = Vec::new();
    let mut search = 0;
    loop {
        let th = find_open_tag_fast(row_inner, search, "th");
        let td = find_open_tag_fast(row_inner, search, "td");
        let open = match (th, td) {
            (Some(a), Some(b)) => a.min(b),
            (Some(a), None) | (None, Some(a)) => a,
            (None, None) => break,
        };
        let content_start = match parse_tag(row_inner, open) {
            Some((_, _, end)) => end,
            None => break,
        };
        let lower = row_inner.to_ascii_lowercase();
        let rest = &lower[content_start..];
        let th_close = rest.find("</th");
        let td_close = rest.find("</td");
        // 有關閉標籤就用到它；缺關閉標籤（畸形 HTML）才截到下一個 `<`。
        let close_rel = th_close.into_iter().chain(td_close).min().or_else(|| {
            rest.find('<').and_then(|open_rel| {
                if open_rel == 0 {
                    // 緊接著就是標籤（例如格子內容以巢狀標籤開頭），不是缺關閉標籤。
                    None
                } else {
                    Some(open_rel)
                }
            })
        });
        match close_rel {
            Some(rel) => {
                cells.push(Cell {
                    inner: row_inner[content_start..content_start + rel].to_string(),
                });
                search = content_start + rel;
            }
            None => {
                cells.push(Cell {
                    inner: row_inner[content_start..].to_string(),
                });
                break;
            }
        }
    }
    cells
}

/// 格子 inner HTML → 純文字：`<br>` 轉換行，其餘標籤移除，實體解碼。
/// 不做前後 trim，由呼叫端依用途正規化。
pub fn cell_text(inner: &str) -> String {
    let with_breaks = replace_insensitive(inner, "<br", "\n");
    let stripped = strip_tags(&with_breaks);
    decode_entities(&stripped)
}

/// 任意 HTML 片段 → 保留換行的純文字：`<br>`、區塊標籤結尾轉換行。
pub fn block_text(fragment: &str) -> String {
    let mut s = fragment.to_string();
    for tag in ["br", "p", "div", "tr", "li", "ul", "ol", "hr"] {
        s = replace_insensitive(&s, &format!("<{tag}"), "\n");
        s = replace_insensitive(&s, &format!("</{tag}"), "\n");
    }
    for level in 1..=6 {
        s = replace_insensitive(&s, &format!("<h{level}"), "\n");
        s = replace_insensitive(&s, &format!("</h{level}"), "\n");
    }
    let stripped = strip_tags(&s);
    let decoded = decode_entities(&stripped);
    // 連續 3 個以上換行壓成 2 個，方便閱讀。
    let mut out = String::with_capacity(decoded.len());
    let mut blanks = 0;
    for line in decoded.split('\n') {
        if line.trim().is_empty() {
            blanks += 1;
            if blanks <= 2 {
                out.push('\n');
            }
        } else {
            blanks = 0;
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn replace_insensitive(s: &str, needle_open: &str, replacement: &str) -> String {
    let lower = s.to_ascii_lowercase();
    let needle = needle_open.to_ascii_lowercase();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while let Some(rel) = lower[i..].find(&needle) {
        let pos = i + rel;
        out.push_str(&s[i..pos]);
        out.push_str(replacement);
        // 跳過整個標籤（到 `>` 為止），避免殘留屬性文字。
        let mut end = pos + needle.len();
        let bytes = s.as_bytes();
        let mut quote = None;
        while end < bytes.len() {
            let b = bytes[end];
            if let Some(q) = quote {
                if b == q {
                    quote = None;
                }
            } else if b == b'"' || b == b'\'' {
                quote = Some(b);
            } else if b == b'>' {
                end += 1;
                break;
            }
            end += 1;
        }
        i = end;
    }
    out.push_str(&s[i..]);
    out
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'<' {
            match parse_tag(s, i) {
                Some((_, _, end)) => i = end,
                None => {
                    out.push(bytes[i] as char);
                    i += 1;
                }
            }
        } else {
            // 以字元為單位推進，保持 UTF-8 完整。
            let ch = s[i..].chars().next().unwrap_or('\u{FFFD}');
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// 常見 HTML 實體解碼（譜面中的 `&lt;`/`&gt;` 必須還原，否則 Slide 方向會壞）。
pub fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        if bytes[i] != b'&' {
            let ch = s[i..].chars().next().unwrap_or('\u{FFFD}');
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }
        let rest = &s[i..];
        let end = rest.find(';').filter(|&p| p <= 10);
        match end {
            Some(p) => {
                let entity = &rest[1..p];
                let decoded = if entity.starts_with("#x") || entity.starts_with("#X") {
                    u32::from_str_radix(&entity[2..], 16)
                        .ok()
                        .and_then(char::from_u32)
                } else if let Some(num) = entity.strip_prefix('#') {
                    num.parse::<u32>().ok().and_then(char::from_u32)
                } else {
                    match entity {
                        "amp" => Some('&'),
                        "lt" => Some('<'),
                        "gt" => Some('>'),
                        "quot" => Some('"'),
                        "apos" | "#39" => Some('\''),
                        "nbsp" => Some(' '),
                        _ => None,
                    }
                };
                match decoded {
                    Some(ch) => {
                        out.push(ch);
                        i += p + 1;
                    }
                    None => {
                        // 未知實體：原樣保留，不丟資料。
                        out.push_str(&rest[..p + 1]);
                        i += p + 1;
                    }
                }
            }
            None => {
                out.push('&');
                i += 1;
            }
        }
    }
    out
}

/// 標題文字正規化：trim＋連續空白（含換行）壓成一個半形空白。
pub fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 抽取全部 h1–h6（不依賴 id/class，只看標題文字）。
pub fn extract_headings(html: &str) -> Vec<Heading> {
    let cleaned = strip_ignored(html);
    let s = cleaned.as_str();
    let lower = s.to_ascii_lowercase();
    let mut headings = Vec::new();
    let mut search = 0;
    while search < s.len() {
        let rel = match lower[search..].find("<h") {
            Some(r) => r,
            None => break,
        };
        let pos = search + rel;
        let level_byte = match s.as_bytes().get(pos + 2) {
            Some(b) => *b,
            None => break,
        };
        if !(b'1'..=b'6').contains(&level_byte) {
            search = pos + 2;
            continue;
        }
        // `<hr` 之類的下一個字元是字母，不是分隔符。
        match s.as_bytes().get(pos + 3) {
            Some(b) if b.is_ascii_alphanumeric() => {
                search = pos + 3;
                continue;
            }
            _ => {}
        }
        let level = level_byte - b'0';
        let content_start = match parse_tag(s, pos) {
            Some((_, _, end)) => end,
            None => break,
        };
        let close_tag = format!("</h{level}");
        let close_rel = match lower[content_start..].find(&close_tag) {
            Some(r) => r,
            None => break,
        };
        let close_pos = content_start + close_rel;
        let content_end = match parse_tag(s, close_pos) {
            Some((_, _, end)) => end,
            None => break,
        };
        headings.push(Heading {
            level,
            text: collapse_ws(&cell_text(&s[content_start..close_pos])),
            tag_start: pos,
            content_end,
        });
        search = content_end;
    }
    headings
}

/// 抽取 inner HTML 中的全部連結。
pub fn extract_links(inner: &str) -> Vec<Link> {
    let lower = inner.to_ascii_lowercase();
    let mut links = Vec::new();
    let mut search = 0;
    while search < inner.len() {
        let rel = match lower[search..].find("<a") {
            Some(r) => r,
            None => break,
        };
        let pos = search + rel;
        match inner.as_bytes().get(pos + 2) {
            Some(b) if b.is_ascii_whitespace() || *b == b'>' => {}
            _ => {
                search = pos + 2;
                continue;
            }
        }
        let content_start = match parse_tag(inner, pos) {
            Some((_, _, end)) => end,
            None => break,
        };
        let tag_text = &inner[pos..content_start];
        let href = match parse_href(tag_text) {
            Some(h) => h,
            None => {
                search = content_start;
                continue;
            }
        };
        let close_rel = match lower[content_start..].find("</a") {
            Some(r) => r,
            None => break,
        };
        let close_pos = content_start + close_rel;
        let close_end = match parse_tag(inner, close_pos) {
            Some((_, _, end)) => end,
            None => break,
        };
        links.push(Link {
            href,
            text: collapse_ws(&cell_text(&inner[content_start..close_pos])),
        });
        search = close_end;
    }
    links
}

fn parse_href(tag_text: &str) -> Option<String> {
    let lower = tag_text.to_ascii_lowercase();
    let mut search = 0;
    loop {
        let rel = lower[search..].find("href")?;
        let mut i = search + rel + 4;
        let bytes = tag_text.as_bytes();
        // `href` 必須是完整屬性名（前面是空白或 `<a`）。
        let prev = if search + rel == 0 {
            b' '
        } else {
            bytes[search + rel - 1]
        };
        if !prev.is_ascii_whitespace() {
            search += rel + 4;
            continue;
        }
        while bytes.get(i).is_some_and(|b| b.is_ascii_whitespace()) {
            i += 1;
        }
        if bytes.get(i) != Some(&b'=') {
            search = i;
            continue;
        }
        i += 1;
        while bytes.get(i).is_some_and(|b| b.is_ascii_whitespace()) {
            i += 1;
        }
        let quote = bytes.get(i);
        if quote == Some(&b'"') || quote == Some(&b'\'') {
            let q = quote.unwrap_or(&b'"');
            i += 1;
            let start = i;
            while bytes.get(i).is_some_and(|b| b != q) {
                i += 1;
            }
            return Some(tag_text[start..i].to_string());
        }
        let start = i;
        while bytes
            .get(i)
            .is_some_and(|b| !b.is_ascii_whitespace() && *b != b'>')
        {
            i += 1;
        }
        if start < i {
            return Some(tag_text[start..i].to_string());
        }
        return None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_tables_without_relying_on_classes() {
        let html = r#"<div class="atwiki-outer"><table class="atwiki-table">
<tr><th>TITLE</th><th>MAS</th></tr>
<tr><td><a href="/simai/pages/592.html">前前前世</a></td>
    <td><a href="/simai/pages/592.html#master">13</a></td></tr>
</table></div>"#;
        let tables = extract_tables(html);
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].rows.len(), 2);
        assert_eq!(cell_text(&tables[0].rows[1].cells[0].inner), "前前前世");
        let links = extract_links(&tables[0].rows[1].cells[1].inner);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].href, "/simai/pages/592.html#master");
        assert_eq!(links[0].text, "13");
    }

    #[test]
    fn decodes_chart_entities() {
        assert_eq!(
            decode_entities("1p4[16:5]*&lt;5[16:5]"),
            "1p4[16:5]*<5[16:5]"
        );
        assert_eq!(decode_entities("A &amp; B"), "A & B");
        assert_eq!(decode_entities("&#65;&#x42;"), "AB");
        assert_eq!(cell_text("(172)<br>{4}1,2,<br>E"), "(172)\n{4}1,2,\nE");
    }

    #[test]
    fn extracts_headings_by_text() {
        let html = r#"<h2 id="id_abc123">MASTER</h2><div>chart</div><h2>Re:MASTER</h2>"#;
        let headings = extract_headings(html);
        assert_eq!(headings.len(), 2);
        assert_eq!(headings[0].level, 2);
        assert_eq!(headings[0].text, "MASTER");
        assert!(headings[0].content_end > headings[0].tag_start);
    }

    #[test]
    fn block_text_preserves_line_breaks() {
        let text = block_text("<div>(172)<br />{4}1,2,</div><br /><div>E</div>");
        assert!(text.contains("(172)\n"));
        assert!(text.contains("{4}1,2,"));
    }
}
