//! 本機歌曲搜尋：只查記憶體／快取中的 `Vec<WikiSong>`，
//! 不在每次按鍵時請求 Wiki。

use super::model::{ChartType, WikiSong};

/// 搜尋文字正規化：trim、全形 ASCII→半形、全形空白→半形、Unicode 小寫、空白壓縮。
pub fn normalize_search_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.trim().chars() {
        let half = match ch {
            '\u{3000}' => ' ',
            // U+FF01–FF5E 全形 ASCII。
            '\u{FF01}'..='\u{FF5E}' => char::from_u32(ch as u32 - 0xFEE0).unwrap_or(ch),
            _ => ch,
        };
        for lower in half.to_lowercase() {
            out.push(lower);
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 子字串搜尋（多詞以空白分隔，全部都要命中）。
/// 排序：完全一致 → 前綴 → 子字串；同級依歌名長度、page_id、種類決定順序。
pub fn search_songs_filtered(
    songs: &[WikiSong],
    query: &str,
    chart_type: Option<ChartType>,
) -> Vec<WikiSong> {
    let query = normalize_search_text(query);
    if query.is_empty() {
        return Vec::new();
    }
    let words: Vec<&str> = query.split(' ').collect();
    let mut ranked: Vec<(u8, usize, u32, u8, &WikiSong)> = songs
        .iter()
        .filter(|song| chart_type.is_none_or(|t| song.chart_type == t))
        .filter_map(|song| {
            let title = normalize_search_text(&song.title);
            if title.is_empty() {
                return None;
            }
            if !words.iter().all(|word| title.contains(word)) {
                return None;
            }
            let rank = if title == query {
                0
            } else if title.starts_with(&query) {
                1
            } else {
                2
            };
            Some((rank, title.len(), song.page_id, song.chart_type as u8, song))
        })
        .collect();
    ranked.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(a.1.cmp(&b.1))
            .then(a.2.cmp(&b.2))
            .then(a.3.cmp(&b.3))
    });
    ranked
        .into_iter()
        .map(|(_, _, _, _, song)| song.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn song(title: &str, page_id: u32, chart_type: ChartType) -> WikiSong {
        WikiSong {
            page_id,
            title: title.into(),
            chart_type,
            page_url: format!("https://w.atwiki.jp/simai/pages/{page_id}.html"),
            difficulties: HashMap::new(),
            section: None,
        }
    }

    fn index() -> Vec<WikiSong> {
        vec![
            song("前前前世", 592, ChartType::Standard),
            song("前前前世", 1592, ChartType::Deluxe),
            song("ようこそジャパリパークへ", 811, ChartType::Standard),
        ]
    }

    #[test]
    fn normalizes_search_text() {
        assert_eq!(normalize_search_text("  前前  "), "前前");
        assert_eq!(normalize_search_text("ＡＢＣ１２"), "abc12");
        assert_eq!(normalize_search_text("MASTER　１３"), "master 13");
        assert_eq!(normalize_search_text("Re:MASTER"), "re:master");
    }

    #[test]
    fn ranks_exact_before_prefix_before_substring() {
        let songs = index();
        let results = search_songs_filtered(&songs, "前前前世", None);
        assert_eq!(results.len(), 2);
        // 完全一致在前；同名 Standard（page_id 小）在前。
        assert_eq!(results[0].page_id, 592);
        assert_eq!(results[1].page_id, 1592);

        let results = search_songs_filtered(&songs, "前前", None);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].page_id, 592);
    }

    #[test]
    fn filters_by_chart_type_without_merging() {
        let songs = index();
        let standard = search_songs_filtered(&songs, "前前前世", Some(ChartType::Standard));
        assert_eq!(standard.len(), 1);
        assert_eq!(standard[0].chart_type, ChartType::Standard);
        let deluxe = search_songs_filtered(&songs, "前前前世", Some(ChartType::Deluxe));
        assert_eq!(deluxe.len(), 1);
        assert_eq!(deluxe[0].chart_type, ChartType::Deluxe);
    }

    #[test]
    fn empty_query_matches_nothing() {
        let songs = index();
        assert!(search_songs_filtered(&songs, "   ", None).is_empty());
    }
}
