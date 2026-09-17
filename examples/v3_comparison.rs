//! Reproducible model comparison, including a long chart; prints measured JSON.
use mai_motion_core::*;
use std::time::Instant;
fn main() {
    let long = format!(
        "(180){{8}}{}E",
        (0..1000)
            .map(|i| format!("{},", [1, 8, 2, 7, 3, 6, 4, 5][i % 8]))
            .collect::<String>()
    );
    let mut rows = vec![];
    for (name, source) in [
        ("slow-pair", "(120){4}1,1,E"),
        ("dense-jack", "(240){24}1,1,1,1,E"),
        ("abab", "(240){24}1,2,1,2,1,2,E"),
        ("1000-taps", long.as_str()),
    ] {
        for config in [
            SolverConfig::default(),
            SolverConfig::v2(),
            SolverConfig::v3(),
        ] {
            let request = AnalyzeRequest {
                request_id: name.into(),
                source: source.into(),
                first_seconds: 0.,
                solver_config: config,
            };
            let model = request.solver_config.scoring_model.clone();
            let start = Instant::now();
            let response = analyze_chart(request);
            let milliseconds = start.elapsed().as_secs_f64() * 1000.;
            let solution = response.solutions.first();
            let hands = solution.map(|s| {
                s.assignments
                    .iter()
                    .take(24)
                    .map(|a| if a.hand == Hand::L { 'L' } else { 'R' })
                    .collect::<String>()
            });
            rows.push(serde_json::json!({"case":name,"model":model,"status":response.status,"milliseconds":milliseconds,"firstHands":hands,"score":solution.and_then(|s|s.score.as_ref()),"scoreBreakdown":solution.and_then(|s|s.score_breakdown.as_ref()),"diagnostics":response.diagnostics}));
            assert_eq!(response.status, "ok");
        }
    }
    println!("{}", serde_json::to_string_pretty(&rows).unwrap());
}
