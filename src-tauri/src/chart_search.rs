//! simai Wiki＋adxdls 合併搜尋：兩邊各自的本機索引一起查，同一首歌合成一筆。
//!
//! 配對規則：同種類（Standard／DX）且歌名正規化後相同、兩邊各恰好一筆才合併。
//! 同名多首（例如兩首不同的「Link」）不猜，各自列出。搜尋不連網。

use crate::adx::{self, AdxSong, AdxState};
use crate::wiki::{self, normalize_search_text, ChartType, WikiSong, WikiState};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use tauri::AppHandle;

const MAX_QUERY_CHARS: usize = 100;

/// 一首歌；wiki 與 adx 至少一邊有值。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartSearchHit {
    pub title: String,
    pub chart_type: ChartType,
    pub wiki: Option<WikiSong>,
    pub adx: Option<AdxSong>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartSearchPayload {
    pub items: Vec<ChartSearchHit>,
    /// 這次有沒有查到該來源的索引（沒載入過也沒有快取時為 false）。
    pub wiki_ready: bool,
    pub adx_ready: bool,
}

/// 命令層入口：驗證參數，在兩邊已載入的索引上搜尋；兩邊都沒有索引才報錯。
pub fn search(
    app: &AppHandle,
    wiki_state: &WikiState,
    adx_state: &AdxState,
    query: &str,
    chart_type: Option<&str>,
) -> Result<ChartSearchPayload, String> {
    let query = query.trim();
    let count = query.chars().count();
    if count == 0 {
        return Err("請輸入要搜尋的歌曲名稱".into());
    }
    if count > MAX_QUERY_CHARS {
        return Err(format!("搜尋文字不可超過 {MAX_QUERY_CHARS} 個字元"));
    }
    let filter = match chart_type.map(str::trim).filter(|s| !s.is_empty()) {
        None | Some("all") => None,
        Some(name) => Some(ChartType::from_param(name).ok_or("譜面種類篩選無效")?),
    };
    let run = |wiki_songs: &[WikiSong]| {
        adx::with_songs(app, adx_state, |adx_songs| {
            (merge_search(wiki_songs, adx_songs, query, filter), true)
        })
        .unwrap_or_else(|| (merge_search(wiki_songs, &[], query, filter), false))
    };
    let ((items, adx_ready), wiki_ready) = match wiki::with_index(app, wiki_state, run) {
        Some(result) => (result, true),
        None => (run(&[]), false),
    };
    if !wiki_ready && !adx_ready {
        return Err("尚未載入 simai Wiki 與 adxdls 的索引，請先重新整理".into());
    }
    Ok(ChartSearchPayload {
        items,
        wiki_ready,
        adx_ready,
    })
}

/// 配對用的歌名鍵：搜尋正規化後去掉所有空白。
fn match_key(title: &str) -> String {
    normalize_search_text(title).replace(' ', "")
}

/// 單一欄位對整串關鍵字的命中等級：完全一致 0、前綴 2、其他 4。
/// 留出奇數給 adxdls 的非歌名欄位，讓它們排在同級的歌名命中之後。
fn closeness(field: &str, query: &str) -> u8 {
    if field == query {
        0
    } else if field.starts_with(query) {
        2
    } else {
        4
    }
}

/// Wiki 只有歌名可查；每個詞都要出現在歌名裡。
fn wiki_rank(song: &WikiSong, query: &str, words: &[&str]) -> Option<u8> {
    let title = normalize_search_text(&song.title);
    words
        .iter()
        .all(|word| title.contains(word))
        .then(|| closeness(&title, query))
}

/// adxdls 另有羅馬拼音、曲師與別名。歌名命中最優先，其他欄位排在同級歌名之後；
/// 每個詞分散在不同欄位也算命中，但排最後。
fn adx_rank(song: &AdxSong, query: &str, words: &[&str]) -> Option<u8> {
    let mut fields: Vec<(String, u8)> = vec![(normalize_search_text(&song.title), 0)];
    for extra in [&song.title_romaji, &song.artist_romaji]
        .into_iter()
        .flatten()
        .chain([&song.artist])
        .chain(&song.aliases)
    {
        fields.push((normalize_search_text(extra), 1));
    }
    let best = fields
        .iter()
        .filter(|(text, _)| words.iter().all(|word| text.contains(word)))
        .map(|(text, offset)| offset + closeness(text, query))
        .min();
    best.or_else(|| {
        words
            .iter()
            .all(|word| fields.iter().any(|(text, _)| text.contains(word)))
            .then_some(6)
    })
}

/// 純計算的合併搜尋（不碰狀態，方便測試）。排序：命中等級 → 歌名長度 → 種類 → 來源編號。
pub fn merge_search(
    wiki_songs: &[WikiSong],
    adx_songs: &[AdxSong],
    query: &str,
    filter: Option<ChartType>,
) -> Vec<ChartSearchHit> {
    let query = normalize_search_text(query);
    if query.is_empty() {
        return Vec::new();
    }
    let words: Vec<&str> = query.split(' ').collect();
    let keep = |chart_type: ChartType| filter.is_none_or(|wanted| wanted == chart_type);

    let mut groups: HashMap<(ChartType, String), (Vec<usize>, Vec<usize>)> = HashMap::new();
    for (index, song) in wiki_songs
        .iter()
        .enumerate()
        .filter(|(_, s)| keep(s.chart_type))
    {
        groups
            .entry((song.chart_type, match_key(&song.title)))
            .or_default()
            .0
            .push(index);
    }
    for (index, song) in adx_songs
        .iter()
        .enumerate()
        .filter(|(_, s)| keep(s.chart_type))
    {
        groups
            .entry((song.chart_type, match_key(&song.title)))
            .or_default()
            .1
            .push(index);
    }
    let mut partner_of_wiki: HashMap<usize, usize> = HashMap::new();
    let mut paired_adx: HashSet<usize> = HashSet::new();
    for (wiki_ids, adx_ids) in groups.values() {
        if let ([wiki_id], [adx_id]) = (wiki_ids.as_slice(), adx_ids.as_slice()) {
            partner_of_wiki.insert(*wiki_id, *adx_id);
            paired_adx.insert(*adx_id);
        }
    }

    // (等級, 歌名字數, 種類, Wiki page id, adx slug 數值, 命中)
    let mut ranked: Vec<(u8, usize, ChartType, u32, u64, ChartSearchHit)> = Vec::new();
    let mut push = |rank: u8, wiki: Option<&WikiSong>, adx: Option<&AdxSong>| {
        let title = adx
            .map(|s| &s.title)
            .or(wiki.map(|s| &s.title))
            .cloned()
            .unwrap_or_default();
        let chart_type = adx
            .map(|s| s.chart_type)
            .or(wiki.map(|s| s.chart_type))
            .unwrap_or(ChartType::Standard);
        ranked.push((
            rank,
            title.chars().count(),
            chart_type,
            wiki.map_or(u32::MAX, |s| s.page_id),
            adx.map_or(u64::MAX, |s| s.slug.parse().unwrap_or(u64::MAX - 1)),
            ChartSearchHit {
                title,
                chart_type,
                wiki: wiki.cloned(),
                adx: adx.cloned(),
            },
        ));
    };
    for (index, song) in wiki_songs
        .iter()
        .enumerate()
        .filter(|(_, s)| keep(s.chart_type))
    {
        let partner = partner_of_wiki.get(&index).map(|&id| &adx_songs[id]);
        let rank = [
            wiki_rank(song, &query, &words),
            partner.and_then(|adx| adx_rank(adx, &query, &words)),
        ]
        .into_iter()
        .flatten()
        .min();
        if let Some(rank) = rank {
            push(rank, Some(song), partner);
        }
    }
    for (index, song) in adx_songs
        .iter()
        .enumerate()
        .filter(|(_, s)| keep(s.chart_type))
    {
        if paired_adx.contains(&index) {
            continue;
        }
        if let Some(rank) = adx_rank(song, &query, &words) {
            push(rank, None, Some(song));
        }
    }
    ranked.sort_by(|a, b| (a.0, a.1, a.2, a.3, a.4).cmp(&(b.0, b.1, b.2, b.3, b.4)));
    ranked.into_iter().map(|item| item.5).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn wiki(title: &str, page_id: u32, chart_type: ChartType) -> WikiSong {
        WikiSong {
            page_id,
            title: title.into(),
            chart_type,
            page_url: format!("https://w.atwiki.jp/simai/pages/{page_id}.html"),
            difficulties: HashMap::new(),
            section: None,
        }
    }

    fn adx(title: &str, slug: &str, chart_type: ChartType, aliases: &[&str]) -> AdxSong {
        AdxSong {
            slug: slug.into(),
            title: title.into(),
            title_romaji: None,
            artist: "artist".into(),
            artist_romaji: None,
            aliases: aliases.iter().map(|alias| alias.to_string()).collect(),
            chart_type,
            bpm: None,
            version: None,
            genre: None,
            difficulties: HashMap::new(),
            page_url: format!("https://adxdls.saop.cc/charts/{slug}"),
        }
    }

    fn sources() -> (Vec<WikiSong>, Vec<AdxSong>) {
        (
            vec![
                wiki("スイートマジック", 121, ChartType::Standard),
                wiki("前前前世", 592, ChartType::Standard),
                wiki("前前前世", 1592, ChartType::Deluxe),
                wiki("Link", 10, ChartType::Standard),
                wiki("Wiki Only", 7, ChartType::Deluxe),
            ],
            vec![
                adx(
                    "スイートマジック",
                    "121",
                    ChartType::Standard,
                    &["甜蜜魔法"],
                ),
                adx("前前前世", "11000", ChartType::Deluxe, &[]),
                adx("Link", "200", ChartType::Standard, &[]),
                adx("Link", "300", ChartType::Standard, &[]),
                adx("ＡＤＸ　Only", "12061", ChartType::Deluxe, &[]),
            ],
        )
    }

    fn summary(hits: &[ChartSearchHit]) -> Vec<(String, Option<u32>, Option<String>)> {
        hits.iter()
            .map(|hit| {
                (
                    hit.title.clone(),
                    hit.wiki.as_ref().map(|s| s.page_id),
                    hit.adx.as_ref().map(|s| s.slug.clone()),
                )
            })
            .collect()
    }

    #[test]
    fn merges_same_title_and_type() {
        let (w, a) = sources();
        let hits = merge_search(&w, &a, "前前前世", None);
        assert_eq!(
            summary(&hits),
            [
                ("前前前世".into(), Some(592), None),
                ("前前前世".into(), Some(1592), Some("11000".into())),
            ]
        );
        assert_eq!(hits[1].chart_type, ChartType::Deluxe);
    }

    #[test]
    fn alias_hit_brings_wiki_partner() {
        let (w, a) = sources();
        let hits = merge_search(&w, &a, "甜蜜魔法", None);
        assert_eq!(
            summary(&hits),
            [("スイートマジック".into(), Some(121), Some("121".into()))]
        );
    }

    #[test]
    fn ambiguous_titles_stay_separate() {
        let (w, a) = sources();
        let hits = merge_search(&w, &a, "link", None);
        assert_eq!(
            summary(&hits),
            [
                ("Link".into(), Some(10), None),
                ("Link".into(), None, Some("200".into())),
                ("Link".into(), None, Some("300".into())),
            ]
        );
    }

    #[test]
    fn single_source_entries_and_width_normalization() {
        let (w, a) = sources();
        assert_eq!(
            summary(&merge_search(&w, &a, "adx only", None)),
            [("ＡＤＸ　Only".into(), None, Some("12061".into()))]
        );
        assert_eq!(
            summary(&merge_search(&w, &a, "wiki", None)),
            [("Wiki Only".into(), Some(7), None)]
        );
    }

    #[test]
    fn filters_by_chart_type() {
        let (w, a) = sources();
        let hits = merge_search(&w, &a, "前前前世", Some(ChartType::Deluxe));
        assert_eq!(
            summary(&hits),
            [("前前前世".into(), Some(1592), Some("11000".into()))]
        );
        assert!(merge_search(&w, &a, "  ", None).is_empty());
    }

    #[test]
    fn title_hits_rank_before_artist_hits() {
        let mut a = vec![adx("Zeta", "1", ChartType::Deluxe, &[])];
        a[0].artist = "Alpha Beta".into();
        a.push(adx("Alpha Song", "2", ChartType::Deluxe, &[]));
        let hits = merge_search(&[], &a, "alpha", None);
        assert_eq!(
            summary(&hits),
            [
                ("Alpha Song".into(), None, Some("2".into())),
                ("Zeta".into(), None, Some("1".into())),
            ]
        );
    }
}
