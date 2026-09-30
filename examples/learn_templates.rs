//! 從整包真人標註學出分組認知的手法樣板（樂句形狀 → 手法），輸出 JSON。
//! 出貨的表：src/solver/v3/templates.json。
//!
//! cargo run --release --example learn_templates -- 整包.json 輸出.json [--exclude 0,3] [--min 3] [--share 0.6]
//!
//! 只用確定、非預填的接觸標註；樂句內每顆都有標註才計入。同一形狀出現至少 min 次，
//! 且最常見的手法佔 share 以上才收錄。
use mai_motion_core::*;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

fn main() {
    let mut args = std::env::args().skip(1);
    let bundle_path = args.next().expect(
        "用法：learn_templates <整包.json> <輸出.json> [--exclude 0,3] [--min 3] [--share 0.6]",
    );
    let out = args.next().expect("缺少輸出路徑");
    let mut exclude: Vec<usize> = vec![];
    let mut min_count = 3usize;
    let mut min_share = 0.6f64;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--exclude" => {
                exclude = args
                    .next()
                    .unwrap()
                    .split(',')
                    .map(|s| s.parse().unwrap())
                    .collect()
            }
            "--min" => min_count = args.next().unwrap().parse().unwrap(),
            "--share" => min_share = args.next().unwrap().parse().unwrap(),
            other => panic!("不認得的參數 {other}"),
        }
    }
    let bundle: Value =
        serde_json::from_str(&std::fs::read_to_string(&bundle_path).expect("讀不到整包檔"))
            .expect("整包檔格式錯誤");
    let mut counts: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for (index, item) in bundle["items"]
        .as_array()
        .expect("沒有 items")
        .iter()
        .enumerate()
    {
        if exclude.contains(&index) {
            continue;
        }
        let annotation = &item["annotation"];
        let source = annotation["chart"]["source"].as_str().unwrap_or_default();
        let first = annotation["chart"]["firstSeconds"].as_f64().unwrap_or(0.);
        let Ok(parsed) = parse_chart(source, first) else {
            continue;
        };
        let chart = parsed.chart;
        let mut hands: HashMap<&str, char> = HashMap::new();
        for mark in annotation["notes"].as_array().into_iter().flatten() {
            let sure = mark["confidence"].as_str().unwrap_or("sure") == "sure"
                && !mark["prefilled"].as_bool().unwrap_or(false);
            if let (Some(key), Some(hand)) = (mark["key"].as_str(), mark["hand"].as_str()) {
                if sure {
                    hands.insert(key, if hand == "L" { 'L' } else { 'R' });
                }
            }
        }
        let by_id: HashMap<&str, &Note> = chart.notes.iter().map(|n| (n.id.as_str(), n)).collect();
        for (signature, ids) in phrase_shapes(&chart) {
            let Some(labels) = ids
                .iter()
                .map(|id| hands.get(by_id[id.as_str()].key.as_str()).copied())
                .collect::<Option<Vec<char>>>()
            else {
                continue;
            };
            let pattern: String = labels
                .iter()
                .map(|h| if *h == labels[0] { 'A' } else { 'B' })
                .collect();
            *counts
                .entry(signature)
                .or_default()
                .entry(pattern)
                .or_default() += 1;
        }
    }
    let mut table: BTreeMap<String, String> = BTreeMap::new();
    for (signature, patterns) in &counts {
        let total: usize = patterns.values().sum();
        let (best, count) = patterns
            .iter()
            .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
            .unwrap();
        if total >= min_count && *count as f64 >= min_share * total as f64 {
            table.insert(signature.clone(), best.clone());
        }
    }
    eprintln!("形狀 {}，收錄樣板 {}", counts.len(), table.len());
    std::fs::write(out, serde_json::to_string_pretty(&table).unwrap() + "\n").expect("寫不出樣板");
}
