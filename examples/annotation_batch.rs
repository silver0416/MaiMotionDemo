//! 一次比對整包真人標註（`*.maimotion-hands-bundle.json`）與模型 Top-1，印出各譜面與整體吻合率。
//! 只求模型解，不求照標註的解；分支中確定的手順也算可接受的答案（另列一欄）。
//!
//! cargo run --release --example annotation_batch -- 整包.json [--config 設定.json] [--tuning 權重.json] [--only 0,3] [--json] [--misses 不吻合.json]
use mai_motion_core::*;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Default, Clone, Copy)]
struct Tally {
    compared: usize,
    agreed: usize,
    agreed_any_branch: usize,
    contact_compared: usize,
    contact_agreed: usize,
}
impl Tally {
    fn add(&mut self, o: Tally) {
        self.compared += o.compared;
        self.agreed += o.agreed;
        self.agreed_any_branch += o.agreed_any_branch;
        self.contact_compared += o.contact_compared;
        self.contact_agreed += o.contact_agreed;
    }
}

fn sure(mark: &Value, field: &str) -> bool {
    let fallback = mark.get("confidence").and_then(Value::as_str);
    let value = if field == "trackConfidence" {
        mark.get(field).and_then(Value::as_str).or(fallback)
    } else {
        fallback
    };
    value.unwrap_or("sure") == "sure"
        && !mark
            .get("prefilled")
            .and_then(Value::as_bool)
            .unwrap_or(false)
}

/// 模型在某顆音符上的手（與 annotation::model_hands 相同規則）。
fn model_hands(solution: &Solution, note_id: &str) -> (Option<&'static str>, Option<&'static str>) {
    let name = |h: Hand| if h == Hand::L { "L" } else { "R" };
    let list: Vec<_> = solution
        .assignments
        .iter()
        .filter(|a| a.note_id == note_id)
        .collect();
    let contact = list
        .iter()
        .filter(|a| a.part != "slide")
        .min_by(|a, b| a.start_seconds.total_cmp(&b.start_seconds))
        .map(|a| name(a.hand));
    let slides: Vec<_> = list.iter().filter(|a| a.part == "slide").collect();
    let track = slides
        .iter()
        .min_by(|a, b| a.start_seconds.total_cmp(&b.start_seconds))
        .map(|first| {
            let both = slides.iter().any(|a| {
                a.hand != first.hand
                    && a.start_seconds < first.end_seconds
                    && first.start_seconds < a.end_seconds
                    && !solution.handovers.iter().any(|h| {
                        h.note_id == note_id && (h.start_seconds - a.start_seconds).abs() < 1e-9
                    })
            });
            if both {
                "LR"
            } else {
                name(first.hand)
            }
        });
    (contact, track)
}

type Evaluated = Result<(Tally, Vec<Value>), String>;

/// 回傳統計與不吻合的項目（主線標註與模型不同，即使分支可接受也列出）。
fn evaluate(annotation: &Value, config: &SolverConfig) -> Evaluated {
    let chart = &annotation["chart"];
    let response = analyze_chart(AnalyzeRequest {
        request_id: "batch".into(),
        source: chart["source"].as_str().unwrap_or_default().into(),
        first_seconds: chart["firstSeconds"].as_f64().unwrap_or(0.),
        solver_config: config.clone(),
    });
    let (Some(parsed), Some(model)) = (&response.chart, response.solutions.first()) else {
        return Err(format!(
            "{}: {:?}",
            response.status,
            response.diagnostics.first().map(|d| &d.message)
        ));
    };
    // 分支中確定的手順：key → 可接受的 (部分, 手)。
    let mut accepted: HashMap<&str, HashSet<(&str, &str)>> = HashMap::new();
    for branch in annotation["branches"].as_array().into_iter().flatten() {
        for mark in branch["notes"].as_array().into_iter().flatten() {
            let key = mark["key"].as_str().unwrap_or_default();
            if let Some(h) = mark["hand"].as_str().filter(|_| sure(mark, "confidence")) {
                accepted.entry(key).or_default().insert(("hand", h));
            }
            if let Some(h) = mark["track"]
                .as_str()
                .filter(|_| sure(mark, "trackConfidence"))
            {
                accepted.entry(key).or_default().insert(("track", h));
            }
        }
    }
    let by_key: HashMap<&str, &Note> = parsed.notes.iter().map(|n| (n.key.as_str(), n)).collect();
    let mut t = Tally::default();
    let mut misses = vec![];
    for mark in annotation["notes"].as_array().into_iter().flatten() {
        let key = mark["key"].as_str().unwrap_or_default();
        let Some(note) = by_key.get(key) else {
            continue;
        };
        let (contact, track) = model_hands(model, &note.id);
        let mut compare = |part: &'static str, human: &str, model: Option<&str>, t: &mut Tally| {
            let Some(model) = model else { return };
            t.compared += 1;
            let ok = human == model;
            if part == "hand" {
                t.contact_compared += 1;
                t.contact_agreed += ok as usize;
            }
            t.agreed += ok as usize;
            let branch = accepted
                .get(key)
                .is_some_and(|s| s.contains(&(part, model)));
            if ok || branch {
                t.agreed_any_branch += 1;
            }
            if !ok {
                misses.push(serde_json::json!({"key": key, "noteId": note.id, "part": part, "human": human, "model": model, "branch": branch}));
            }
        };
        if let Some(h) = mark["hand"].as_str().filter(|_| sure(mark, "confidence")) {
            compare("hand", h, contact, &mut t);
        }
        if note.path_id.is_some() {
            if let Some(h) = mark["track"]
                .as_str()
                .filter(|_| sure(mark, "trackConfidence"))
            {
                compare("track", h, track, &mut t);
            }
        }
    }
    Ok((t, misses))
}

fn main() {
    let mut args = std::env::args().skip(1);
    let bundle_path = args.next().expect("用法：annotation_batch <整包.json> [--config f] [--tuning f] [--only 0,3] [--json] [--misses f]");
    let mut config = SolverConfig::v3();
    let mut only: Option<Vec<usize>> = None;
    let mut json = false;
    let mut misses_path: Option<String> = None;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--config" => {
                let text = std::fs::read_to_string(args.next().unwrap()).expect("讀不到設定");
                config = serde_json::from_str(&text).expect("設定格式錯誤");
            }
            "--tuning" => {
                let text = std::fs::read_to_string(args.next().unwrap()).expect("讀不到權重");
                let tuning = serde_json::from_str(&text).expect("權重格式錯誤");
                scoring_v3::set_tuning(tuning);
            }
            "--only" => {
                only = Some(
                    args.next()
                        .unwrap()
                        .split(',')
                        .map(|s| s.parse().unwrap())
                        .collect(),
                );
            }
            "--json" => json = true,
            "--misses" => misses_path = args.next(),
            other => panic!("不認得的參數 {other}"),
        }
    }
    let bundle: Value =
        serde_json::from_str(&std::fs::read_to_string(&bundle_path).expect("讀不到整包檔"))
            .expect("整包檔格式錯誤");
    let items: Vec<(usize, &Value)> = bundle["items"]
        .as_array()
        .expect("整包檔沒有 items")
        .iter()
        .map(|item| &item["annotation"])
        .enumerate()
        .filter(|(i, _)| only.as_ref().is_none_or(|o| o.contains(i)))
        .collect();
    let results: Vec<(usize, String, Evaluated)> = std::thread::scope(|scope| {
        let handles: Vec<_> = items
            .iter()
            .map(|(i, a)| {
                let config = &config;
                scope.spawn(move || {
                    (
                        *i,
                        a["title"].as_str().unwrap_or("?").to_string(),
                        evaluate(a, config),
                    )
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut total = Tally::default();
    let pct = |a: usize, b: usize| {
        if b == 0 {
            0.
        } else {
            100. * a as f64 / b as f64
        }
    };
    let mut rows = vec![];
    let mut all_misses = vec![];
    for (i, title, result) in &results {
        match result {
            Ok((t, misses)) => {
                total.add(*t);
                all_misses.push(serde_json::json!({"index": i, "misses": misses}));
                rows.push(serde_json::json!({"index": i, "title": title, "compared": t.compared, "agreed": t.agreed, "agreedAnyBranch": t.agreed_any_branch, "contactCompared": t.contact_compared, "contactAgreed": t.contact_agreed}));
                if !json {
                    println!(
                        "{i} {:<34} {:>5.1}%  含分支 {:>5.1}%  接觸 {:>5.1}%  ({}/{})",
                        title,
                        pct(t.agreed, t.compared),
                        pct(t.agreed_any_branch, t.compared),
                        pct(t.contact_agreed, t.contact_compared),
                        t.agreed,
                        t.compared
                    );
                }
            }
            Err(e) => {
                rows.push(serde_json::json!({"index": i, "title": title, "error": e}));
                if !json {
                    println!("{i} {title} 失敗：{e}");
                }
            }
        }
    }
    if json {
        println!(
            "{}",
            serde_json::json!({"items": rows, "compared": total.compared, "agreed": total.agreed, "agreedAnyBranch": total.agreed_any_branch, "contactCompared": total.contact_compared, "contactAgreed": total.contact_agreed})
        );
    } else {
        println!(
            "整體 {:.1}%  含分支 {:.1}%  接觸 {:.1}%  ({}/{})",
            pct(total.agreed, total.compared),
            pct(total.agreed_any_branch, total.compared),
            pct(total.contact_agreed, total.contact_compared),
            total.agreed,
            total.compared
        );
    }
    if let Some(path) = misses_path {
        std::fs::write(path, serde_json::to_string(&all_misses).unwrap())
            .expect("寫不出不吻合清單");
    }
}
