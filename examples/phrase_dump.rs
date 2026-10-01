//! 列出整包標記檔每首譜面的樂句（分組認知的自動切法），輸出 JSON 供分析。
//!
//! cargo run --release --example phrase_dump -- 整包.json > 樂句.json
use mai_motion_core::*;
use serde_json::{json, Value};
use std::collections::HashMap;

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("用法：phrase_dump <整包.json>");
    let bundle: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("讀不到整包檔"))
            .expect("整包檔格式錯誤");
    let mut out = vec![];
    for (index, item) in bundle["items"]
        .as_array()
        .expect("沒有 items")
        .iter()
        .enumerate()
    {
        let annotation = &item["annotation"];
        let source = annotation["chart"]["source"].as_str().unwrap_or_default();
        let first = annotation["chart"]["firstSeconds"].as_f64().unwrap_or(0.);
        let Ok(parsed) = parse_chart(source, first) else {
            continue;
        };
        let chart = parsed.chart;
        let by_id: HashMap<&str, &Note> = chart.notes.iter().map(|n| (n.id.as_str(), n)).collect();
        let phrases: Vec<Value> = phrase_shapes(&chart)
            .into_iter()
            .map(|(signature, ids)| {
                let notes: Vec<&Note> = ids.iter().map(|id| by_id[id.as_str()]).collect();
                json!({
                    "signature": signature,
                    "keys": notes.iter().map(|n| n.key.clone()).collect::<Vec<_>>(),
                    "times": notes.iter().map(|n| n.time_seconds).collect::<Vec<_>>(),
                    "buttons": notes.iter().map(|n| n.button).collect::<Vec<_>>(),
                })
            })
            .collect();
        out.push(json!({ "index": index, "phrases": phrases }));
    }
    println!("{}", serde_json::to_string(&out).unwrap());
}
