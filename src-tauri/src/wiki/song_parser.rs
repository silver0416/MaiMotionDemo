//! 個別歌曲頁（`pages/{id}.html`）解析。
//!
//! - Metadata：掃描全部 table，找含 `アーティスト` 的列與含 `BPM` 的欄位列，
//!   不依賴第幾個 table。
//! - 難度小節：只認 h1–h6 標題文字（EASY/BASIC/ADVANCED/EXPERT/MASTER/Re:MASTER），
//!   不認 heading id。從標題後的 sibling 收集文字，直到下一個難度標題或
//!   同級／更高級標題。
//! - Wiki 層只產出 raw simai 字串，不解讀 tap/slide/hold 語法。

use super::html::{self, Heading};
use super::model::{Difficulty, WikiChartSection, WikiError, WikiSongPage};
use std::collections::HashMap;

/// 解析歌曲頁全文。
pub fn parse_song_page(html: &str, page_id: u32) -> Result<WikiSongPage, WikiError> {
    // 全程使用清理後的同一份字串，heading 偏移與切片才會對齊。
    let cleaned = html::cleaned_html(html);
    let html = cleaned.as_str();
    let headings = html::extract_headings(html);
    let sections = difficulty_sections(&headings);
    if sections.is_empty() {
        return Err(WikiError::InvalidSongPage(
            "歌曲頁找不到難度小節，版面可能已變更".into(),
        ));
    }
    let title = song_title(
        &headings,
        sections.first().map(|s| s.heading_index).unwrap_or(0),
    );
    let tables = html::extract_tables(html);
    let artist = parse_artist(&tables);
    let bpm = parse_bpm(&tables);
    let mut charts = HashMap::new();
    for section in &sections {
        let end = section_end(html, &headings, section);
        let fragment = html.get(section.content_end..end).unwrap_or_default();
        let raw_text = clean_chart_text(&html::block_text(fragment));
        let available =
            !raw_text.trim().is_empty() && raw_text.contains('{') && raw_text.contains('}');
        charts.insert(
            section.difficulty,
            WikiChartSection {
                difficulty: section.difficulty,
                raw_text,
                available,
            },
        );
    }
    Ok(WikiSongPage {
        page_id,
        title,
        artist,
        bpm,
        charts,
    })
}

struct Section {
    difficulty: Difficulty,
    heading_index: usize,
    #[allow(dead_code)]
    level: u8,
    content_end: usize,
}

fn difficulty_sections(headings: &[Heading]) -> Vec<Section> {
    headings
        .iter()
        .enumerate()
        .filter_map(|(index, heading)| {
            Difficulty::from_section_text(&heading.text).map(|difficulty| Section {
                difficulty,
                heading_index: index,
                level: heading.level,
                content_end: heading.content_end,
            })
        })
        .collect()
}

/// 小節結束：下一個難度標題，或下一個同級／更高級標題，否則文件結尾。
fn section_end(html: &str, headings: &[Heading], section: &Section) -> usize {
    let current = &headings[section.heading_index];
    for next in &headings[section.heading_index + 1..] {
        if Difficulty::from_section_text(&next.text).is_some() {
            return next.tag_start;
        }
        if next.level <= current.level {
            return next.tag_start;
        }
    }
    html.len()
}

/// 歌名：第一個難度標題之前、最後一個非難度、非空的標題。
fn song_title(headings: &[Heading], first_section_index: usize) -> String {
    headings[..first_section_index]
        .iter()
        .rev()
        .find(|heading| {
            !heading.text.is_empty() && Difficulty::from_section_text(&heading.text).is_none()
        })
        .map(|heading| heading.text.clone())
        .unwrap_or_default()
}

/// 找含 `アーティスト` 的列，取其下一個格子。
fn parse_artist(tables: &[super::html::Table]) -> Option<String> {
    for table in tables {
        for row in &table.rows {
            let texts: Vec<String> = row
                .cells
                .iter()
                .map(|cell| html::collapse_ws(&html::cell_text(&cell.inner)))
                .collect();
            if let Some(pos) = texts.iter().position(|text| text == "アーティスト") {
                if let Some(value) = texts.get(pos + 1).filter(|v| !v.is_empty()) {
                    return Some(value.clone());
                }
            }
        }
    }
    None
}

/// 找含 `BPM` 的欄位列，取第一筆資料列 BPM 欄下方的值。
fn parse_bpm(tables: &[super::html::Table]) -> Option<f64> {
    for table in tables {
        for (index, row) in table.rows.iter().enumerate() {
            let texts: Vec<String> = row
                .cells
                .iter()
                .map(|cell| html::collapse_ws(&html::cell_text(&cell.inner)))
                .collect();
            let bpm_col = match texts
                .iter()
                .position(|text| text.eq_ignore_ascii_case("BPM"))
            {
                Some(col) => col,
                None => continue,
            };
            for data_row in &table.rows[index + 1..] {
                let values: Vec<String> = data_row
                    .cells
                    .iter()
                    .map(|cell| html::collapse_ws(&html::cell_text(&cell.inner)))
                    .collect();
                if let Some(value) = values.get(bpm_col) {
                    if let Some(bpm) = parse_bpm_value(value) {
                        return Some(bpm);
                    }
                }
            }
        }
    }
    None
}

fn parse_bpm_value(text: &str) -> Option<f64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if let Ok(value) = text.parse::<f64>() {
        return finite(value);
    }
    // 容忍 `190～200` 這類範圍寫法：取開頭數字。
    let prefix: String = text
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    if prefix.is_empty() {
        return None;
    }
    prefix.parse::<f64>().ok().and_then(finite)
}

fn finite(value: f64) -> Option<f64> {
    value.is_finite().then_some(value)
}

/// 整理抽出的譜面文字：CRLF→LF、行尾 trim、去開頭空白、截到最後的結尾 `E`。
/// 不改動 tap/slide/hold/touch/BPM/小節語法。
pub fn clean_chart_text(raw: &str) -> String {
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines: Vec<&str> = normalized.lines().collect();
    // 去開頭純空白行（保留行尾 trim 後的內容）。
    let first_content = lines
        .iter()
        .position(|line| !line.trim().is_empty())
        .unwrap_or(lines.len());
    lines = lines[first_content..].to_vec();
    // 找到最後一個獨立成行的結尾 `E`，後面的 Wiki 留言直接捨棄。
    if let Some(last_e) = lines.iter().rposition(|line| line.trim() == "E") {
        lines.truncate(last_e + 1);
    }
    // 去結尾空白行。
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    lines
        .iter()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SONG_FIXTURE: &str = r#"
<h2>Test Song</h2>
<table>
<tr><td>アーティスト</td><td>Test Artist</td></tr>
</table>
<table>
<tr><td>BPM</td><td>BASIC</td><td>ADVANCED</td><td>EXPERT</td>
    <td>MASTER</td><td>Re:MASTER</td></tr>
<tr><td>172</td><td>3</td><td>7</td><td>10</td><td>13</td><td>-</td></tr>
</table>
<h2>BASIC</h2>
<h2>ADVANCED</h2>
<h2>EXPERT</h2>
<p>
(172)<br>
{4}1,2,3,4,<br>
E
</p>
<h2>MASTER</h2>
<div>
(172)<br>
{8}1,2,3,4,5,6,7,8,<br>
E
</div>
<h2>Re:MASTER</h2>
<table>
<tr><td>名前:</td><td></td></tr>
<tr><td>コメント:</td><td>よろしくお願いします</td></tr>
</table>
"#;

    fn chart(page: &WikiSongPage, difficulty: Difficulty) -> &WikiChartSection {
        page.charts.get(&difficulty).unwrap()
    }

    #[test]
    fn parses_song_metadata() {
        let page = parse_song_page(SONG_FIXTURE, 1889).unwrap();
        assert_eq!(page.page_id, 1889);
        assert_eq!(page.title, "Test Song");
        assert_eq!(page.artist.as_deref(), Some("Test Artist"));
        assert_eq!(page.bpm, Some(172.0));
    }

    #[test]
    fn empty_sections_are_unavailable() {
        let page = parse_song_page(SONG_FIXTURE, 1889).unwrap();
        assert!(!chart(&page, Difficulty::Basic).available);
        assert!(!chart(&page, Difficulty::Advanced).available);
        assert!(chart(&page, Difficulty::Expert).available);
        assert!(chart(&page, Difficulty::Master).available);
        // Re:MASTER 只有留言板文字、沒有譜面記號：不可用。
        assert!(!chart(&page, Difficulty::ReMaster).available);
    }

    #[test]
    fn extracts_chart_text_verbatim() {
        let page = parse_song_page(SONG_FIXTURE, 1889).unwrap();
        let expert = &chart(&page, Difficulty::Expert).raw_text;
        assert!(expert.contains("(172)"));
        assert!(expert.contains("{4}"));
        assert!(expert.trim_end().ends_with('E'));
        let master = &chart(&page, Difficulty::Master).raw_text;
        assert!(master.contains("{8}1,2,3,4,5,6,7,8,"));
    }

    #[test]
    fn decodes_slide_entities() {
        let html = "<h2>Song</h2><h2>MASTER</h2><div>{8}1p4[16:5]*&lt;5[16:5],<br>E</div>";
        let page = parse_song_page(html, 1).unwrap();
        let master = &chart(&page, Difficulty::Master).raw_text;
        assert!(master.contains("1p4[16:5]*<5[16:5]"));
        assert!(master.contains('{'));
        assert!(chart(&page, Difficulty::Master).available);
    }

    #[test]
    fn truncates_after_terminating_e() {
        let cleaned = clean_chart_text("(172)\n{4}1,2,\nE\n後面的留言\n更多文字");
        assert_eq!(cleaned, "(172)\n{4}1,2,\nE");
    }

    #[test]
    fn rejects_pages_without_difficulty_sections() {
        let html = "<h2>Some Page</h2><p>沒有譜面</p>";
        assert!(matches!(
            parse_song_page(html, 1),
            Err(WikiError::InvalidSongPage(_))
        ));
    }

    /// Live 測試：預設不執行。592 MASTER 有譜；1889 BASIC/ADVANCED 為空、
    /// EXPERT/MASTER 有譜。
    #[test]
    #[ignore]
    fn live_song_pages_parse() {
        tauri::async_runtime::block_on(async {
            let client = crate::wiki::client::build_client().expect("wiki client");
            let standard = crate::wiki::client::fetch_song_html(&client, 592)
                .await
                .expect("fetch page 592");
            let page = parse_song_page(&standard, 592).unwrap();
            assert_eq!(page.title, "前前前世");
            assert_eq!(page.artist.as_deref(), Some("RADWIMPS"));
            assert_eq!(page.bpm, Some(190.0));
            assert!(page.charts.get(&Difficulty::Master).unwrap().available);

            let deluxe = crate::wiki::client::fetch_song_html(&client, 1889)
                .await
                .expect("fetch page 1889");
            let page = parse_song_page(&deluxe, 1889).unwrap();
            assert!(!page.charts.get(&Difficulty::Basic).unwrap().available);
            assert!(!page.charts.get(&Difficulty::Advanced).unwrap().available);
            assert!(page.charts.get(&Difficulty::Expert).unwrap().available);
            assert!(page.charts.get(&Difficulty::Master).unwrap().available);
        });
    }
}
