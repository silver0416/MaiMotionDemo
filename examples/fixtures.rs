use mai_motion_core::*;
fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "fixtures".into());
    std::fs::create_dir_all(&out).unwrap();
    for (name, source) in [
        ("tap", "(120){4}1,2,3,4,5,6,7,8,E"),
        ("hold", "(120){4}8h[4:4]/1,2,3,4,E"),
        ("slide", "(120){4}1-5[4:3],8,7,6,E"),
        ("handover", "(120){4}8>4[1##3],,,,,,,8,E"),
        ("unsupported", "(120){4}C,E"),
        ("no-solution", "(120){4}1/4/7,E"),
    ] {
        let mut config = SolverConfig::default();
        if name == "handover" {
            config.handover_weight = 0.05;
            config.side_weight = 12.0;
        }
        if name == "slide" {
            config.allow_handover = false;
        }
        let request = AnalyzeRequest {
            request_id: name.into(),
            source: source.into(),
            first_seconds: 0.0,
            solver_config: config,
        };
        let response = analyze_chart(request.clone());
        std::fs::write(
            format!("{out}/{name}.json"),
            serde_json::to_string_pretty(
                &serde_json::json!({"request":request,"response":response}),
            )
            .unwrap(),
        )
        .unwrap();
        println!(
            "{name}: {}, handovers={}",
            response.status,
            response.solutions.first().map_or(0, |s| s.handovers.len())
        );
    }
}
