//! 分組認知：依節奏把按鍵切成樂句。形狀（各組的相對鍵位與節奏，左右鏡像視為
//! 同一種）相同的樂句，玩家傾向用同一套手法；手法樣板由玩家標註學出
//! （examples/learn_templates.rs），出貨的表為 templates.json。
use super::*;
use std::sync::OnceLock;

/// 超過這個間隔一定分組。
const PHRASE_MAX_GAP: f64 = 0.4;
/// 間隔比前一個間隔長這麼多倍時分組。
const PHRASE_BREAK_RATIO: f64 = 1.6;
const PHRASE_MAX_NOTES: usize = 8;

/// 一個樂句：形狀簽名與依簽名順序排列的音符。
pub(crate) struct Phrase<'a> {
    pub signature: String,
    pub notes: Vec<&'a Note>,
}

/// 按鍵接觸（含 Slide 起點、和弦），依時間分組後切成樂句。
pub(crate) fn phrases(chart: &Chart) -> Vec<Phrase<'_>> {
    let mut contacts: Vec<&Note> = chart
        .notes
        .iter()
        .filter(|n| n.touch_area.is_none() && (n.kind != "slide" || n.has_head))
        .collect();
    contacts.sort_by(|a, b| {
        a.time_seconds
            .total_cmp(&b.time_seconds)
            .then(a.button.cmp(&b.button))
    });
    let mut groups: Vec<Vec<&Note>> = vec![];
    for note in contacts {
        match groups.last_mut() {
            Some(g) if (note.time_seconds - g[0].time_seconds).abs() < 0.002 => g.push(note),
            _ => groups.push(vec![note]),
        }
    }
    let mut result = vec![];
    let mut current: Vec<Vec<&Note>> = vec![];
    for group in groups {
        if let Some(last) = current.last() {
            let gap = group[0].time_seconds - last[0].time_seconds;
            let previous = (current.len() >= 2)
                .then(|| last[0].time_seconds - current[current.len() - 2][0].time_seconds);
            if gap > PHRASE_MAX_GAP || previous.is_some_and(|p| gap > PHRASE_BREAK_RATIO * p) {
                result.extend(phrase(&current));
                current.clear();
            }
        }
        current.push(group);
    }
    result.extend(phrase(&current));
    result
}

fn phrase<'a>(groups: &[Vec<&'a Note>]) -> Option<Phrase<'a>> {
    let count: usize = groups.iter().map(Vec::len).sum();
    if groups.len() < 2 || count > PHRASE_MAX_NOTES {
        return None;
    }
    let base = groups[0][0].button;
    let times: Vec<f64> = groups.iter().map(|g| g[0].time_seconds).collect();
    let first_gap = times[1] - times[0];
    let rhythm: Vec<String> = times
        .windows(2)
        .map(|w| ((w[1] - w[0]) / first_gap * 4.0).round().to_string())
        .collect();
    // 兩個方向各自排好組內順序，取字串較小者為標準方向。
    let oriented = |mirror: bool| {
        let mut notes = vec![];
        let mut parts = vec![];
        for group in groups {
            let mut rel: Vec<(u8, &Note)> = group
                .iter()
                .map(|n| {
                    let r = (n.button + 8 - base) % 8;
                    (if mirror { (8 - r) % 8 } else { r }, *n)
                })
                .collect();
            rel.sort_by_key(|(r, _)| *r);
            parts.push(
                rel.iter()
                    .map(|(r, _)| r.to_string())
                    .collect::<Vec<_>>()
                    .join("+"),
            );
            notes.extend(rel.into_iter().map(|(_, n)| n));
        }
        (parts.join(","), notes)
    };
    let (normal, normal_notes) = oriented(false);
    let (mirrored, mirrored_notes) = oriented(true);
    let (shape, notes) = if mirrored < normal {
        (mirrored, mirrored_notes)
    } else {
        (normal, normal_notes)
    };
    Some(Phrase {
        signature: format!("{shape}|{}", rhythm.join(",")),
        notes,
    })
}

/// 手法：第一顆的手記為 A，同手 A、另一手 B。
pub(crate) fn pattern(hands: &[Hand]) -> String {
    hands
        .iter()
        .map(|h| if *h == hands[0] { 'A' } else { 'B' })
        .collect()
}

static TEMPLATES: OnceLock<BTreeMap<String, String>> = OnceLock::new();

/// 換一張樣板表（評估用）；已經讀過時回傳 false。
pub fn set_templates(templates: BTreeMap<String, String>) -> bool {
    TEMPLATES.set(templates).is_ok()
}

pub(crate) fn templates() -> &'static BTreeMap<String, String> {
    TEMPLATES.get_or_init(|| {
        serde_json::from_str(include_str!("templates.json")).expect("templates.json 格式錯誤")
    })
}

/// 樂句的形狀簽名與音符 id（簽名順序），供學習樣板。
pub fn phrase_shapes(chart: &Chart) -> Vec<(String, Vec<String>)> {
    phrases(chart)
        .into_iter()
        .map(|p| (p.signature, p.notes.iter().map(|n| n.id.clone()).collect()))
        .collect()
}
