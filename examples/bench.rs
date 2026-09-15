use mai_motion_core::*;
use std::time::Instant;

/// 產生 n 顆音符的合成譜面，每 8 顆插一條 Slide，接近真實譜面的密度。
fn chart_source(n: usize) -> String {
    let mut s = String::from("(180){8}");
    for i in 0..n {
        let k = (i % 8) + 1;
        if i % 8 == 7 {
            let end = ((k + 3) % 8) + 1;
            s.push_str(&format!("{k}-{end}[8:1],"));
        } else {
            s.push_str(&format!("{k},"));
        }
    }
    s.push('E');
    s
}

fn main() {
    for n in [400usize, 800, 1600, 3200] {
        let source = chart_source(n);
        let clock = Instant::now();
        let r = analyze_chart(AnalyzeRequest {
            request_id: "bench".into(),
            source: source.clone(),
            first_seconds: 0.0,
            solver_config: SolverConfig::default(),
        });
        let elapsed = clock.elapsed();
        println!(
            "notes={n:5} bytes={:7} status={:12} {:?}",
            source.len(),
            r.status,
            elapsed
        );
        if r.status != "ok" {
            println!("        {}", r.diagnostics[0].message);
        }
    }
}
