//! 列出模型解與照標註解每一段移動（距離、時間、速度成本、前後在做什麼），供分析
//! 人類接受哪些快速移動。
//!
//! cargo run --release --example motion_dump -- 整包.json [--only 0,3] > 移動.json
use mai_motion_core::scoring_v3::{PreferenceConfigV3, ScoringV3};
use mai_motion_core::*;
use serde_json::{json, Value};

fn segments(solution: &Solution, engine: &ScoringV3) -> Vec<Value> {
    let mut out = vec![];
    for (hand, list) in [
        (Hand::L, &solution.left_segments),
        (Hand::R, &solution.right_segments),
    ] {
        for (i, s) in list.iter().enumerate() {
            if !matches!(s.mode.as_str(), "travel" | "glide") {
                continue;
            }
            let d: f64 = s
                .samples
                .windows(2)
                .map(|p| p[0].point().distance(p[1].point()))
                .sum();
            let cost = engine
                .motion_terms(s, hand)
                .map(|t| t.speed_strain)
                .unwrap_or(f64::NAN);
            let around = |j: Option<usize>| {
                j.and_then(|j| list.get(j)).map(|s| {
                    json!({"mode": s.mode, "noteId": s.note_id, "start": s.start_seconds, "end": s.end_seconds})
                })
            };
            // 之前最後一段非移動、之後第一段非移動（idle 也算，代表等待）。
            let prev = (0..i)
                .rev()
                .find(|&j| !matches!(list[j].mode.as_str(), "travel"));
            let next = (i + 1..list.len()).find(|&j| !matches!(list[j].mode.as_str(), "travel"));
            out.push(json!({
                "hand": if hand == Hand::L { "L" } else { "R" },
                "mode": s.mode,
                "start": s.start_seconds,
                "end": s.end_seconds,
                "from": [s.samples[0].x, s.samples[0].y],
                "to": [s.samples.last().unwrap().x, s.samples.last().unwrap().y],
                "d": d,
                "cost": cost,
                "prev": around(prev),
                "next": around(next),
            }));
        }
    }
    out
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .expect("用法：motion_dump <整包.json> [--only 0,3]");
    let mut only: Option<Vec<usize>> = None;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--only" => {
                only = Some(
                    args.next()
                        .unwrap()
                        .split(',')
                        .map(|s| s.parse().unwrap())
                        .collect(),
                )
            }
            other => panic!("不認得的參數 {other}"),
        }
    }
    let bundle: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("讀不到整包檔"))
            .expect("整包檔格式錯誤");
    let config = SolverConfig::v3();
    let engine = ScoringV3::new(PreferenceConfigV3::default()).unwrap();
    let items: Vec<(usize, HandAnnotation)> = bundle["items"]
        .as_array()
        .expect("沒有 items")
        .iter()
        .enumerate()
        .filter(|(i, _)| only.as_ref().is_none_or(|o| o.contains(i)))
        .map(|(i, item)| {
            (
                i,
                serde_json::from_value(item["annotation"].clone()).expect("標註格式錯誤"),
            )
        })
        .collect();
    let out: Vec<Value> = std::thread::scope(|scope| {
        let handles: Vec<_> = items
            .iter()
            .map(|(index, annotation)| {
                let (config, engine) = (&config, &engine);
                scope.spawn(move || {
                    let source = annotation.chart.source.clone();
                    let first = annotation.chart.first_seconds;
                    let model = analyze_chart(AnalyzeRequest {
                        request_id: "dump".into(),
                        source: source.clone(),
                        first_seconds: first,
                        solver_config: config.clone(),
                    });
                    let evaluated = evaluate_annotation(EvaluateRequest {
                        request_id: "dump".into(),
                        source,
                        first_seconds: first,
                        solver_config: config.clone(),
                        annotation: annotation.clone(),
                    });
                    let notes: Vec<Value> = model
                        .chart
                        .as_ref()
                        .map(|c| {
                            c.notes
                                .iter()
                                .map(|n| json!({"id": n.id, "key": n.key, "kind": n.kind, "time": n.time_seconds, "end": n.end_seconds, "button": n.button, "x": n.position.x, "y": n.position.y}))
                                .collect()
                        })
                        .unwrap_or_default();
                    let hands = |s: &Solution| -> Vec<Value> {
                        s.assignments
                            .iter()
                            .map(|a| json!({"noteId": a.note_id, "part": a.part, "hand": if a.hand == Hand::L { "L" } else { "R" }, "start": a.start_seconds, "end": a.end_seconds}))
                            .collect()
                    };
                    json!({
                        "index": index,
                        "notes": notes,
                        "divergences": evaluated.divergences.iter().map(|d| json!({"noteId": d.note_id, "part": d.part})).collect::<Vec<_>>(),
                        "model": model.solutions.first().map(|s| json!({"segments": segments(s, engine), "assignments": hands(s)})),
                        "human": evaluated.human_solution.as_ref().map(|s| json!({"segments": segments(s, engine), "assignments": hands(s)})),
                    })
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    println!("{}", serde_json::to_string(&out).unwrap());
}
