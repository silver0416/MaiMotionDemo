//! 讀取真人標註檔，印出與模型的吻合率、成本差距與分歧音符（調整參數時的參照）。
//!
//! cargo run --release --example annotation_eval -- 標註檔.json [solverConfig.json]
use mai_motion_core::*;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("用法：annotation_eval <標註檔.json> [solverConfig.json]");
        std::process::exit(2);
    };
    let text = std::fs::read_to_string(&path).expect("讀不到標註檔");
    let annotation: HandAnnotation = serde_json::from_str(&text).expect("標註檔格式錯誤");
    let solver_config = match args.next() {
        Some(config) => serde_json::from_str(&std::fs::read_to_string(config).expect("讀不到設定"))
            .expect("設定格式錯誤"),
        None => SolverConfig::v3(),
    };
    let request = EvaluateRequest {
        request_id: "cli".into(),
        source: annotation.chart.source.clone(),
        first_seconds: annotation.chart.first_seconds,
        solver_config,
        annotation,
    };
    let start = std::time::Instant::now();
    let r = evaluate_annotation(request);
    println!(
        "status      {}  ({:.1} s)",
        r.status,
        start.elapsed().as_secs_f64()
    );
    println!(
        "labeled     {}  matched {}  unmatched {}",
        r.labeled,
        r.matched,
        r.unmatched_keys.len()
    );
    if r.compared > 0 {
        println!(
            "agreement   {}/{} = {:.1}%",
            r.agreed,
            r.compared,
            100.0 * r.agreed as f64 / r.compared as f64
        );
    }
    if let (Some(model), Some(human)) = (&r.model, &r.human) {
        println!(
            "cost        model {:.4}  human {:.4}  gap {:+.4}",
            model.cost,
            human.cost,
            human.cost - model.cost
        );
    }
    for d in &r.diagnostics {
        println!(
            "diagnostic  {} {:?} {:?} {}",
            d.code, d.time_seconds, d.note_ids, d.message
        );
    }
    for d in &r.divergences {
        println!(
            "diverge     {:>9.3}s {:<24} {:<5} human {:?} model {:?}",
            d.time_seconds, d.key, d.part, d.human, d.model
        );
    }
    for key in &r.unmatched_keys {
        println!("unmatched   {key}");
    }
}
