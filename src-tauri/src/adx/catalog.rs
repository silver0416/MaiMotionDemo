//! 從 `/charts` 頁抽出完整目錄。
//!
//! 站方沒有公開 JSON API：頁面以 `self.__next_f.push([1,"…"])` 分段送出 Next.js RSC 串流，
//! 把字串接起來後，目錄是其中一個 `[{"id":…,"slug":…,"difficulties":[…]}, …]` 陣列。
//! 這裡不認 RSC 的列號或欄位順序，只找「元素帶 slug、title、difficulties 的最長陣列」，
//! 版面小改仍讀得到；真的讀不到或筆數過少時回報錯誤，由 provider 保留舊快取。

use crate::wiki::{ChartType, Difficulty};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// 目錄頁位址；歌曲頁是 `/charts/{slug}`。
pub const SITE_ROOT: &str = "https://adxdls.saop.cc";
/// 解析出的 Standard＋DX 歌曲低於此數視為版面異常（2026-10 約 1780 首）。
pub const MIN_CATALOG_SONGS: usize = 300;

/// 某個難度的定數與譜師；定數是字串（`12.8`、`14+?`），不轉數字。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdxDifficultyInfo {
    pub level: Option<String>,
    pub designer: Option<String>,
}

/// adxdls 目錄的一首歌。Standard 與 DX 是兩筆；宴譜不收（simai Wiki 也沒有對應難度）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdxSong {
    /// 站內代號，也是譜面路徑 `/adxcs/{slug}/maidata.txt` 的一段。
    pub slug: String,
    pub title: String,
    pub title_romaji: Option<String>,
    pub artist: String,
    pub artist_romaji: Option<String>,
    /// 站方整理的別名（中文譯名、縮寫等），只用於搜尋。
    pub aliases: Vec<String>,
    pub chart_type: ChartType,
    pub bpm: Option<f64>,
    pub version: Option<String>,
    pub genre: Option<String>,
    pub difficulties: HashMap<Difficulty, AdxDifficultyInfo>,
    pub page_url: String,
}

/// slug 只允許英數、底線與連字號，避免拼出站外或上層路徑。
pub fn is_valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 64
        && slug
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// maidata 的 `&inote_N` 編號：1 = Easy … 6 = Re:Master（7 是宴，不收）。
pub fn inote_slot(difficulty: Difficulty) -> u32 {
    match difficulty {
        Difficulty::Easy => 1,
        Difficulty::Basic => 2,
        Difficulty::Advanced => 3,
        Difficulty::Expert => 4,
        Difficulty::Master => 5,
        Difficulty::ReMaster => 6,
    }
}

fn difficulty_of_slot(slot: u64) -> Option<Difficulty> {
    Difficulty::ALL
        .into_iter()
        .find(|difficulty| u64::from(inote_slot(*difficulty)) == slot)
}

/// 解析整頁 HTML。成功時依 (種類, slug) 排序，確保輸出穩定。
pub fn parse_catalog_html(html: &str) -> Result<Vec<AdxSong>, String> {
    let stream = rsc_stream(html);
    // 沒有 RSC 分段時退回直接掃整頁（例如站方改成內嵌 JSON）。
    let text = if stream.is_empty() { html } else { &stream };
    let mut best: Vec<AdxSong> = Vec::new();
    for value in object_arrays(text) {
        visit_arrays(&value, &mut |array| {
            let songs: Vec<AdxSong> = array.iter().filter_map(song_from).collect();
            if songs.len() > best.len() {
                best = songs;
            }
        });
    }
    if best.is_empty() {
        return Err("adxdls 目錄頁讀不到歌曲資料，版面可能已變更".into());
    }
    let mut seen = std::collections::HashSet::new();
    best.retain(|song| seen.insert(song.slug.clone()));
    best.sort_by(|a, b| {
        (a.chart_type, slug_order(&a.slug), &a.slug).cmp(&(
            b.chart_type,
            slug_order(&b.slug),
            &b.slug,
        ))
    });
    Ok(best)
}

/// 數字 slug 依數值排（9 在 10 前面），其他排在後面。
fn slug_order(slug: &str) -> u64 {
    slug.parse().unwrap_or(u64::MAX)
}

/// 把所有 `self.__next_f.push([1,"…"])` 的字串依序接起來。
fn rsc_stream(html: &str) -> String {
    const MARKER: &str = "self.__next_f.push(";
    let mut out = String::new();
    let mut rest = html;
    while let Some(at) = rest.find(MARKER) {
        rest = &rest[at + MARKER.len()..];
        let mut values = serde_json::Deserializer::from_str(rest).into_iter::<Value>();
        let Some(Ok(Value::Array(items))) = values.next() else {
            continue;
        };
        if let [Value::Number(kind), Value::String(chunk)] = items.as_slice() {
            if kind.as_u64() == Some(1) {
                out.push_str(chunk);
            }
        }
        rest = &rest[values.byte_offset()..];
    }
    out
}

/// 找出文字裡所有「以物件開頭的 JSON 陣列」。解析成功就跳到陣列結尾，
/// 不會把目錄裡每首歌的 difficulties 小陣列再從文字解析一次（巢狀的交給 visit_arrays）。
fn object_arrays(text: &str) -> Vec<Value> {
    let mut arrays = Vec::new();
    let mut from = 0;
    while let Some(found) = text[from..].find("[{\"") {
        let start = from + found;
        let mut values = serde_json::Deserializer::from_str(&text[start..]).into_iter::<Value>();
        match values.next() {
            Some(Ok(value @ Value::Array(_))) => {
                from = start + values.byte_offset().max(1);
                arrays.push(value);
            }
            _ => from = start + 1,
        }
    }
    arrays
}

/// 走訪值裡的每個陣列（含巢狀），目錄被包在別的物件裡也找得到。
fn visit_arrays(value: &Value, visit: &mut impl FnMut(&[Value])) {
    match value {
        Value::Array(items) => {
            visit(items);
            for item in items {
                visit_arrays(item, visit);
            }
        }
        Value::Object(object) => {
            for item in object.values() {
                visit_arrays(item, visit);
            }
        }
        _ => {}
    }
}

fn text_of(object: &Map<String, Value>, key: &str) -> Option<String> {
    let text = match object.get(key)? {
        Value::String(text) => text.trim().to_string(),
        Value::Number(number) => number.to_string(),
        _ => return None,
    };
    (!text.is_empty()).then_some(text)
}

/// 譜師欄的 `-` 是「未標示」。
fn designer_of(object: &Map<String, Value>) -> Option<String> {
    text_of(object, "designer").filter(|name| name != "-")
}

/// 單筆寬鬆轉換：缺欄位或型別不對的條目略過，不讓一筆壞資料拖垮整份目錄。
fn song_from(value: &Value) -> Option<AdxSong> {
    let object = value.as_object()?;
    let slug = text_of(object, "slug")?;
    if !is_valid_slug(&slug) {
        return None;
    }
    let title = text_of(object, "title")?;
    let chart_type = match text_of(object, "cabinet")?.as_str() {
        "DX" => ChartType::Deluxe,
        "SD" => ChartType::Standard,
        _ => return None,
    };
    let difficulties: HashMap<Difficulty, AdxDifficultyInfo> = object
        .get("difficulties")?
        .as_array()?
        .iter()
        .filter_map(|item| {
            let item = item.as_object()?;
            let difficulty = difficulty_of_slot(item.get("slot")?.as_u64()?)?;
            Some((
                difficulty,
                AdxDifficultyInfo {
                    level: text_of(item, "level"),
                    designer: designer_of(item),
                },
            ))
        })
        .collect();
    if difficulties.is_empty() {
        return None;
    }
    let aliases = object
        .get("aliases")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|alias| !alias.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    let bpm = match object.get("bpm") {
        Some(Value::Number(number)) => number.as_f64(),
        Some(Value::String(text)) => text.trim().parse().ok(),
        _ => None,
    }
    .filter(|bpm: &f64| bpm.is_finite() && *bpm > 0.0);
    Some(AdxSong {
        page_url: format!("{SITE_ROOT}/charts/{slug}"),
        slug,
        title,
        title_romaji: text_of(object, "title_romaji"),
        artist: text_of(object, "artist").unwrap_or_default(),
        artist_romaji: text_of(object, "artist_romaji"),
        aliases,
        chart_type,
        bpm,
        version: text_of(object, "version"),
        genre: text_of(object, "genre"),
        difficulties,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 依 2026-10 實際頁面的結構縮小而成：目錄在第三段 push，前面有欄位較少的 initialTracks 陣列。
    fn sample_html() -> String {
        let catalog = r#"16:["$","$L2d",null,{"entries":[{"id":"121","slug":"121","title":"スイートマジック","title_romaji":"suiitomajikku","artist":"ろん×Junky","artist_romaji":"ron×Junky","category":"Remote","version":"maimai PLUS","genre":"niconico＆ボーカロイド","cabinet":"SD","aliases":["甜蜜魔法","swmg"],"bpm":123,"difficulties":[{"slot":2,"name":"Basic","level":"4.0","designer":"-"},{"slot":5,"name":"Master","level":"12.4","designer":"ニャイン"}]},{"id":"12061-xodus","slug":"12061","title":"XODUS","artist":"DJ Myosuke & Gram","cabinet":"DX","bpm":202.5,"difficulties":[{"slot":4,"name":"Expert","level":"12.8","designer":"けんけん法師"},{"slot":6,"name":"Re:Master","level":"14.6","designer":""}]},{"id":"100042-jelly","slug":"100042","title":"[宴]jelly","artist":"x","cabinet":"宴","difficulties":[{"slot":7,"name":"Utage","level":"12+?","designer":""}]},{"id":"bad","slug":"../x","title":"Bad","cabinet":"DX","difficulties":[{"slot":5,"level":"1"}]},{"id":"9","slug":"9","title":"Nine","cabinet":"DX","difficulties":[{"slot":5,"level":13}]}]}]"#;
        let tracks = r#"5:["$","$L1",null,{"initialTracks":[{"id":"10360","slug":"10360","title":"x","artist":"y"}]}]"#;
        let push = |chunk: &str| {
            format!(
                "<script>self.__next_f.push({})</script>",
                serde_json::to_string(&serde_json::json!([1, chunk])).unwrap()
            )
        };
        format!(
            "<html><body>{}<script>self.__next_f.push([0])</script>{}{}</body></html>",
            push(tracks),
            push("1:\"$Sreact.fragment\"\n"),
            push(catalog)
        )
    }

    #[test]
    fn parses_catalog_from_rsc_pushes() {
        let songs = parse_catalog_html(&sample_html()).unwrap();
        let slugs: Vec<&str> = songs.iter().map(|song| song.slug.as_str()).collect();
        // Standard 在前；DX 依 slug 數值排序；宴譜與不安全的 slug 不收。
        assert_eq!(slugs, ["121", "9", "12061"]);

        let sweet = &songs[0];
        assert_eq!(sweet.chart_type, ChartType::Standard);
        assert_eq!(sweet.aliases, ["甜蜜魔法", "swmg"]);
        assert_eq!(sweet.bpm, Some(123.0));
        assert_eq!(sweet.page_url, "https://adxdls.saop.cc/charts/121");
        let basic = &sweet.difficulties[&Difficulty::Basic];
        assert_eq!(basic.level.as_deref(), Some("4.0"));
        assert_eq!(basic.designer, None);
        assert_eq!(
            sweet.difficulties[&Difficulty::Master].designer.as_deref(),
            Some("ニャイン")
        );

        let xodus = &songs[2];
        assert_eq!(xodus.chart_type, ChartType::Deluxe);
        assert_eq!(xodus.bpm, Some(202.5));
        assert!(xodus.difficulties.contains_key(&Difficulty::ReMaster));
        // 數字型定數也收成字串。
        assert_eq!(
            songs[1].difficulties[&Difficulty::Master].level.as_deref(),
            Some("13")
        );
    }

    #[test]
    fn rejects_page_without_catalog() {
        assert!(parse_catalog_html("<html>maintenance</html>").is_err());
    }

    #[test]
    fn slug_allowlist() {
        assert!(is_valid_slug("12061"));
        assert!(is_valid_slug("abc-def_1"));
        assert!(!is_valid_slug(""));
        assert!(!is_valid_slug("../x"));
        assert!(!is_valid_slug("a/b"));
        assert!(!is_valid_slug("x?y"));
    }

    /// 對真實頁面跑一次：`ADX_CATALOG_HTML=<存下來的 /charts HTML> cargo test -- --ignored`。
    #[test]
    #[ignore]
    fn parses_saved_real_page() {
        let path = std::env::var("ADX_CATALOG_HTML").expect("set ADX_CATALOG_HTML");
        let html = std::fs::read_to_string(path).unwrap();
        let songs = parse_catalog_html(&html).unwrap();
        assert!(
            songs.len() >= MIN_CATALOG_SONGS,
            "only {} songs",
            songs.len()
        );
        let xodus = songs.iter().find(|song| song.slug == "12061").unwrap();
        assert_eq!(xodus.title, "XODUS");
        eprintln!("{} songs", songs.len());
    }
}
