//! Index 頁面（Standard `pages/32.html`、DX `pages/808.html`）解析。
//!
//! 不依賴 AtWiki 外層 CSS class 與 table 位置：掃描全部 table，
//! 用欄位語意（TITLE/ESY/BSC/ADV/EXP/MAS/Re:MAS）判斷是否為譜面表。
//! 宴會場（TITLE/属性/LEVEL/COMMENT）直接忽略。

use super::html::{self, Row, Table};
use super::model::{ChartType, Difficulty, WikiDifficultyInfo, WikiError, WikiSong};
use std::collections::HashMap;

const WIKI_ORIGIN: &str = "https://w.atwiki.jp";
const WIKI_PREFIX: &str = "https://w.atwiki.jp/simai/pages/";

/// 單一 row 解析失敗只跳過該 row，不讓整個 index 掛掉。
/// 整頁完全找不到譜面表才回傳錯誤（避免改版後默默回傳 0 首）。
fn schema(chart_type: ChartType) -> &'static [Difficulty] {
    match chart_type {
        ChartType::Standard => &Difficulty::ALL,
        ChartType::Deluxe => &[
            Difficulty::Basic,
            Difficulty::Advanced,
            Difficulty::Expert,
            Difficulty::Master,
            Difficulty::ReMaster,
        ],
    }
}

/// 宴會場表格的欄位：一出現就不是普通難度表。
fn is_utage_headers(normalized: &[String]) -> bool {
    normalized
        .iter()
        .any(|h| h == "属性" || h == "LEVEL" || h == "COMMENT")
}

fn normalize_header(text: &str) -> String {
    html::collapse_ws(text).to_ascii_uppercase()
}

/// 解析單一 index 頁面的全部譜面表。
pub fn parse_index_html(html: &str, chart_type: ChartType) -> Result<Vec<WikiSong>, WikiError> {
    let tables = html::extract_tables(html);
    let mut songs: Vec<WikiSong> = Vec::new();
    let mut matched_tables = 0usize;
    for table in &tables {
        if let Some(parsed) = parse_chart_table(table, chart_type) {
            matched_tables += 1;
            songs.extend(parsed);
        }
    }
    if matched_tables == 0 {
        return Err(WikiError::InvalidIndexPage(
            "simai Wiki 索引頁找不到譜面表格，版面可能已變更".into(),
        ));
    }
    Ok(merge_duplicates(songs))
}

/// 表格的欄位若符合譜面表 schema，回傳該表全部歌曲；否則回傳 None（忽略）。
fn parse_chart_table(table: &Table, chart_type: ChartType) -> Option<Vec<WikiSong>> {
    let difficulties = schema(chart_type);
    // 欄位列不一定是第一列（前面可能有曲風分區列），逐列尋找。
    let header_row = table.rows.iter().position(|row| {
        if row.cells.len() != difficulties.len() + 1 {
            return false;
        }
        let normalized: Vec<String> = row
            .cells
            .iter()
            .map(|c| normalize_header(&html::cell_text(&c.inner)))
            .collect();
        if is_utage_headers(&normalized) {
            return false;
        }
        normalized[0] == "TITLE"
            && normalized[1..]
                .iter()
                .zip(difficulties.iter())
                .all(|(got, difficulty)| got == &difficulty.index_header().to_ascii_uppercase())
    })?;
    // 分區名稱：欄位列之前最近的單格列（例如曲風分類），僅供顯示與除錯。
    let section = table.rows[..header_row]
        .iter()
        .rev()
        .find(|row| row.cells.len() == 1)
        .map(|row| html::collapse_ws(&html::cell_text(&row.cells[0].inner)))
        .filter(|s| !s.is_empty());
    let mut songs = Vec::new();
    for row in &table.rows[header_row + 1..] {
        if row.cells.len() != difficulties.len() + 1 {
            continue;
        }
        if let Some(song) = parse_song_row(row, chart_type, difficulties, section.clone()) {
            songs.push(song);
        }
    }
    Some(songs)
}

fn parse_song_row(
    row: &Row,
    chart_type: ChartType,
    difficulties: &[Difficulty],
    section: Option<String>,
) -> Option<WikiSong> {
    let links = html::extract_links(&row.cells[0].inner);
    let mut title = html::collapse_ws(&html::cell_text(&row.cells[0].inner));
    if title.is_empty() {
        // 格子文字被標籤吃掉時的後備：用第一個連結的文字。
        title = links
            .first()
            .map(|link| link.text.clone())
            .unwrap_or_default();
    }
    if title.is_empty() {
        return None;
    }
    // 一定從 TITLE 格的 <a href> 取真正 URL，不只靠歌名拼 URL。
    let (page_id, page_url) = links.iter().find_map(|link| resolve_song_url(&link.href))?;
    let mut info = HashMap::new();
    for (difficulty, cell) in difficulties.iter().zip(row.cells[1..].iter()) {
        let level = parse_level(&html::cell_text(&cell.inner));
        let links = html::extract_links(&cell.inner);
        let anchor_url = links.iter().find_map(|link| resolve_any_url(&link.href));
        info.insert(
            *difficulty,
            WikiDifficultyInfo {
                level,
                // 格子有連結＝大概率已有文字譜，只是提示。
                availability_hint: !links.is_empty(),
                anchor_url,
            },
        );
    }
    Some(WikiSong {
        page_id,
        title,
        chart_type,
        page_url,
        difficulties: info,
        section,
    })
}

/// `-`、空白視為無等級；`7+`、`13+`、`14?` 原樣保留為字串，不轉數字。
fn parse_level(text: &str) -> Option<String> {
    let level = html::collapse_ws(text);
    match level.as_str() {
        "" | "-" | "－" | "―" | "—" => None,
        _ => Some(level),
    }
}

/// 把 href 解析成 `(page_id, 歌曲頁絕對 URL)`。
/// 只允許 `https://w.atwiki.jp/simai/pages/{數字}.html`（可帶 fragment）。
pub fn resolve_song_url(href: &str) -> Option<(u32, String)> {
    let absolute = resolve_any_url(href)?;
    let rest = absolute.strip_prefix(WIKI_PREFIX)?;
    let (id_part, _) = rest.split_once(".html")?;
    // `.html` 後面只允許結束、`#`、`?`。
    let after = &rest[id_part.len() + ".html".len()..];
    if !after.is_empty() && !after.starts_with('#') && !after.starts_with('?') {
        return None;
    }
    let page_id: u32 = id_part.parse().ok()?;
    Some((page_id, format!("{WIKI_PREFIX}{id_part}.html")))
}

/// 把各種寫法的 href 轉成絕對 URL（不驗證路徑）。
/// 支援絕對 URL、protocol-relative（`//host/...`）、根相對與相對路徑。
pub fn resolve_any_url(href: &str) -> Option<String> {
    let href = href.trim();
    if href.is_empty() {
        return None;
    }
    // 去掉 fragment 後再判斷，保留的 anchor_url 仍含原始 fragment 由呼叫端決定。
    if let Some(rest) = href.strip_prefix("//") {
        return Some(format!("https://{rest}"));
    }
    if href.starts_with("https://") {
        return Some(href.to_string());
    }
    if href.starts_with("http://") {
        return None;
    }
    if let Some(path) = href.strip_prefix('/') {
        return Some(format!("{WIKI_ORIGIN}/{path}"));
    }
    if href.starts_with('#') || href.starts_with("javascript:") {
        return None;
    }
    Some(format!("{WIKI_ORIGIN}/simai/{href}"))
}

/// 依 `(chart_type, page_id)` 去重：同 key 多次出現時合併難度資訊。
pub fn merge_duplicates(songs: Vec<WikiSong>) -> Vec<WikiSong> {
    let mut merged: HashMap<(ChartType, u32), WikiSong> = HashMap::new();
    for song in songs {
        match merged.get_mut(&(song.chart_type, song.page_id)) {
            None => {
                merged.insert((song.chart_type, song.page_id), song);
            }
            Some(existing) => {
                for (difficulty, incoming) in song.difficulties {
                    match existing.difficulties.get_mut(&difficulty) {
                        None => {
                            existing.difficulties.insert(difficulty, incoming);
                        }
                        Some(current) => {
                            if current.level.is_none() {
                                current.level = incoming.level;
                            }
                            current.availability_hint =
                                current.availability_hint || incoming.availability_hint;
                            if current.anchor_url.is_none() {
                                current.anchor_url = incoming.anchor_url;
                            }
                        }
                    }
                }
                // 歌名保留第一次出現的；不同也不覆蓋。
            }
        }
    }
    let mut songs: Vec<WikiSong> = merged.into_values().collect();
    songs.sort_by_key(|song| (song.chart_type, song.page_id));
    songs
}

#[cfg(test)]
mod tests {
    use super::*;

    const STANDARD_FIXTURE: &str = r#"
<table>
  <tr><td colspan="7">POPS＆アニメ</td></tr>
  <tr>
    <th>TITLE</th><th>ESY</th><th>BSC</th><th>ADV</th>
    <th>EXP</th><th>MAS</th><th>Re:MAS</th>
  </tr>
  <tr>
    <td><a href="/simai/pages/592.html">前前前世</a></td>
    <td>2</td>
    <td><a href="/simai/pages/592.html#basic">4</a></td>
    <td><a href="/simai/pages/592.html#advanced">7</a></td>
    <td><a href="/simai/pages/592.html#expert">9</a></td>
    <td><a href="/simai/pages/592.html#master">13</a></td>
    <td>-</td>
  </tr>
  <tr>
    <td><a href="/simai/pages/811.html">ようこそジャパリパークへ</a></td>
    <td>2</td>
    <td><a href="/simai/pages/811.html#basic">5</a></td>
    <td><a href="/simai/pages/811.html#advanced">7+</a></td>
    <td><a href="/simai/pages/811.html#expert">9+</a></td>
    <td><a href="/simai/pages/811.html#master">12</a></td>
    <td><a href="/simai/pages/811.html#remaster">12+</a></td>
  </tr>
</table>
"#;

    const DELUXE_FIXTURE: &str = r#"
<table>
  <tr><td colspan="6">CiRCLE PLUS 新曲</td></tr>
  <tr>
    <th>TITLE</th><th>BSC</th><th>ADV</th><th>EXP</th><th>MAS</th><th>Re:MAS</th>
  </tr>
  <tr>
    <td><a href="/simai/pages/1889.html">相性×優勝ドロップス</a></td>
    <td>3</td>
    <td>7</td>
    <td><a href="/simai/pages/1889.html#expert">10</a></td>
    <td><a href="/simai/pages/1889.html#master">13</a></td>
    <td>-</td>
  </tr>
</table>
"#;

    const UTAGE_FIXTURE: &str = r#"
<table>
  <tr><td colspan="4">宴会場</td></tr>
  <tr><th>TITLE</th><th>属性</th><th>LEVEL</th><th>COMMENT</th></tr>
  <tr>
    <td><a href="/simai/pages/1879.html">[僕]ボックスワンターバ〜ン</a></td>
    <td>僕・バディ</td>
    <td>14?</td>
    <td>楽しいリズムゲーム</td>
  </tr>
</table>
"#;

    fn info(song: &WikiSong, difficulty: Difficulty) -> &WikiDifficultyInfo {
        song.difficulties.get(&difficulty).unwrap()
    }

    #[test]
    fn parses_standard_index_row() {
        let songs = parse_index_html(STANDARD_FIXTURE, ChartType::Standard).unwrap();
        assert_eq!(songs.len(), 2);
        let song = songs.iter().find(|s| s.page_id == 592).unwrap();
        assert_eq!(song.title, "前前前世");
        assert_eq!(song.chart_type, ChartType::Standard);
        assert_eq!(song.page_url, "https://w.atwiki.jp/simai/pages/592.html");

        assert_eq!(info(song, Difficulty::Easy).level.as_deref(), Some("2"));
        assert!(!info(song, Difficulty::Easy).availability_hint);
        assert_eq!(info(song, Difficulty::Basic).level.as_deref(), Some("4"));
        assert!(info(song, Difficulty::Basic).availability_hint);
        assert_eq!(info(song, Difficulty::Advanced).level.as_deref(), Some("7"));
        assert!(info(song, Difficulty::Advanced).availability_hint);
        assert_eq!(info(song, Difficulty::Expert).level.as_deref(), Some("9"));
        assert!(info(song, Difficulty::Expert).availability_hint);
        assert_eq!(info(song, Difficulty::Master).level.as_deref(), Some("13"));
        assert!(info(song, Difficulty::Master).availability_hint);
        // `-` 轉為 None，不是解析錯誤。
        assert_eq!(info(song, Difficulty::ReMaster).level, None);
        assert!(!info(song, Difficulty::ReMaster).availability_hint);
    }

    #[test]
    fn parses_plus_levels_without_error() {
        let songs = parse_index_html(STANDARD_FIXTURE, ChartType::Standard).unwrap();
        let song = songs.iter().find(|s| s.page_id == 811).unwrap();
        assert_eq!(
            info(song, Difficulty::Advanced).level.as_deref(),
            Some("7+")
        );
        assert_eq!(info(song, Difficulty::Expert).level.as_deref(), Some("9+"));
        assert_eq!(
            info(song, Difficulty::ReMaster).level.as_deref(),
            Some("12+")
        );
        assert!(info(song, Difficulty::ReMaster).availability_hint);
    }

    #[test]
    fn parses_deluxe_index_without_easy() {
        let songs = parse_index_html(DELUXE_FIXTURE, ChartType::Deluxe).unwrap();
        assert_eq!(songs.len(), 1);
        let song = &songs[0];
        assert_eq!(song.title, "相性×優勝ドロップス");
        assert_eq!(song.page_id, 1889);
        assert_eq!(song.chart_type, ChartType::Deluxe);
        assert!(!song.difficulties.contains_key(&Difficulty::Easy));
        assert!(!info(song, Difficulty::Basic).availability_hint);
        assert!(!info(song, Difficulty::Advanced).availability_hint);
        assert!(info(song, Difficulty::Expert).availability_hint);
        assert!(info(song, Difficulty::Master).availability_hint);
        assert_eq!(info(song, Difficulty::ReMaster).level, None);
    }

    #[test]
    fn ignores_utage_tables() {
        let result = parse_index_html(UTAGE_FIXTURE, ChartType::Deluxe);
        assert!(matches!(result, Err(WikiError::InvalidIndexPage(_))));
        // 宴會場表格混在正常表格中時只忽略該表。
        let mixed = format!("{DELUXE_FIXTURE}{UTAGE_FIXTURE}");
        let songs = parse_index_html(&mixed, ChartType::Deluxe).unwrap();
        assert_eq!(songs.len(), 1);
        assert_eq!(songs[0].page_id, 1889);
    }

    #[test]
    fn skips_malformed_rows_without_failing_index() {
        let html = r#"
<table>
  <tr><th>TITLE</th><th>BSC</th><th>ADV</th><th>EXP</th><th>MAS</th><th>Re:MAS</th></tr>
  <tr><td>沒有連結的歌</td><td>3</td><td>7</td><td>10</td><td>13</td><td>-</td></tr>
  <tr><td></td><td>3</td><td>7</td><td>10</td><td>13</td><td>-</td></tr>
  <tr><td><a href="/simai/pages/1889.html">相性×優勝ドロップス</a></td>
    <td>3</td><td>7</td>
    <td><a href="/simai/pages/1889.html#expert">10</a></td>
    <td><a href="/simai/pages/1889.html#master">13</a></td>
    <td>-</td></tr>
</table>"#;
        let songs = parse_index_html(html, ChartType::Deluxe).unwrap();
        assert_eq!(songs.len(), 1);
        assert_eq!(songs[0].page_id, 1889);
    }

    #[test]
    fn merges_duplicate_songs() {
        let html = format!("{DELUXE_FIXTURE}{DELUXE_FIXTURE}");
        let songs = parse_index_html(&html, ChartType::Deluxe).unwrap();
        assert_eq!(songs.len(), 1);
        // 合併後 hint 取 OR。
        assert!(
            songs[0]
                .difficulties
                .get(&Difficulty::Expert)
                .unwrap()
                .availability_hint
        );
    }

    #[test]
    fn resolves_song_urls() {
        assert_eq!(
            resolve_song_url("/simai/pages/592.html"),
            Some((592, "https://w.atwiki.jp/simai/pages/592.html".into()))
        );
        assert_eq!(
            resolve_song_url("//w.atwiki.jp/simai/pages/592.html#BASIC"),
            Some((592, "https://w.atwiki.jp/simai/pages/592.html".into()))
        );
        assert_eq!(
            resolve_song_url("https://w.atwiki.jp/simai/pages/592.html#MASTER"),
            Some((592, "https://w.atwiki.jp/simai/pages/592.html".into()))
        );
        // 非歌曲頁不追：編輯、站外、wiki 搜尋。
        assert_eq!(resolve_song_url("/simai/editx/592.html"), None);
        assert_eq!(
            resolve_song_url("https://example.com/simai/pages/592.html"),
            None
        );
        assert_eq!(
            resolve_song_url("https://w.atwiki.jp/simai/?page=592"),
            None
        );
    }

    /// Live 測試：預設不執行（`cargo test -- --ignored` 才跑），不讓 CI 打 Wiki。
    #[test]
    #[ignore]
    fn live_index_pages_parse() {
        tauri::async_runtime::block_on(async {
            let client = crate::wiki::client::build_client().expect("wiki client");
            let standard_html = crate::wiki::client::fetch_standard_index_html(&client)
                .await
                .expect("fetch standard index");
            let standard = parse_index_html(&standard_html, ChartType::Standard).unwrap();
            assert!(standard.len() >= 50, "standard songs: {}", standard.len());
            let zenzente = standard
                .iter()
                .find(|s| s.page_id == 592)
                .expect("前前前世 page 592");
            assert_eq!(zenzente.title, "前前前世");
            assert!(
                zenzente
                    .difficulties
                    .get(&Difficulty::Master)
                    .unwrap()
                    .availability_hint
            );

            let deluxe_html = crate::wiki::client::fetch_deluxe_index_html(&client)
                .await
                .expect("fetch deluxe index");
            let deluxe = parse_index_html(&deluxe_html, ChartType::Deluxe).unwrap();
            assert!(deluxe.len() >= 50, "deluxe songs: {}", deluxe.len());
            assert!(
                deluxe
                    .iter()
                    .find(|s| s.page_id == 1889)
                    .expect("相性×優勝ドロップス page 1889")
                    .difficulties
                    .get(&Difficulty::Master)
                    .unwrap()
                    .availability_hint
            );
        });
    }
}
