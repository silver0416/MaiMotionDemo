//! 比對 Rust 推論與 PyTorch 參考機率（export_model.py --ref 產生）。
//! cargo run --release --example temoto_parity -- <評估包.json> <參考.json> <temoto-1-tokiwa|temoto-1-akane>
use mai_motion_core::temoto::{features, TemotoModel};
use mai_motion_core::*;
use std::collections::HashMap;

fn main() {
    let mut args = std::env::args().skip(1);
    let bundle: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(args.next().unwrap()).unwrap()).unwrap();
    let reference: HashMap<String, serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(args.next().unwrap()).unwrap()).unwrap();
    let id = args.next().unwrap();
    let model = TemotoModel::builtin(&id).expect("模型");
    let config = SolverConfig::v3();
    let (mut worst, mut flips, mut total) = (0f32, 0usize, 0usize);
    for (i, item) in bundle["items"].as_array().unwrap().iter().enumerate() {
        let Some(r) = reference.get(&i.to_string()) else {
            continue;
        };
        let a = &item["annotation"];
        let chart = parse_chart(
            a["chart"]["source"].as_str().unwrap(),
            a["chart"]["firstSeconds"].as_f64().unwrap_or(0.),
        )
        .unwrap()
        .chart;
        let run = || {
            let sol = analyze_chart(AnalyzeRequest {
                request_id: "p".into(),
                source: a["chart"]["source"].as_str().unwrap().into(),
                first_seconds: a["chart"]["firstSeconds"].as_f64().unwrap_or(0.),
                solver_config: config.clone(),
            });
            let input = features(&chart, sol.solutions.first());
            (
                input
                    .order
                    .iter()
                    .map(|&k| chart.notes[k].key.clone())
                    .collect::<Vec<_>>(),
                model.predict(&input),
            )
        };
        let (keys, p) = match model.tuning() {
            Some(t) => scoring_v3::with_tuning(t, run),
            None => run(),
        };
        let rkeys: Vec<String> = r["keys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k.as_str().unwrap().into())
            .collect();
        assert_eq!(keys, rkeys, "第 {i} 首音符順序不同");
        let rp = r["p"].as_array().unwrap();
        let mut song_worst = 0f32;
        for (k, row) in rp.iter().enumerate() {
            for c in 0..2 {
                let want = row[c].as_f64().unwrap() as f32;
                let got = p[k][c];
                song_worst = song_worst.max((want - got).abs());
                total += 1;
                flips += usize::from((want > 0.5) != (got > 0.5));
            }
        }
        worst = worst.max(song_worst);
        println!("{i:>2} 音符 {:>4} 最大差 {song_worst:.2e}", keys.len());
    }
    println!("全部：最大差 {worst:.2e}，左右判斷不同 {flips}/{total}");
}
