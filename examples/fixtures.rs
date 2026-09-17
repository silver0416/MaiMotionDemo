use mai_motion_core::*;
fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "fixtures".into());
    std::fs::create_dir_all(&out).unwrap();
    for (name, source) in [
        ("tap", "(120){4}1,2,3,4,5,6,7,8,E"),
        ("hold", "(120){4}8h[4:4]/1,2,3,4,E"),
        ("slide", "(120){4}1-5[4:3],8,7,6,E"),
        ("handover", "(120){4}8>4[1##3],,,,,,,8,E"),
        ("touch", "(120){4}A1f,B3,Cf,D5f,E7,Chf[4:2],E"),
        ("palm", "(120){4}Chf[4:4]/B1/E1/7,8,E"),
        (
            "shapes",
            "(120){2}1-5[4:1],3^6[4:1],5v2[4:1],7q3[4:1],1s5[4:1],8w4[4:1],2V84[4:1],E",
        ),
        ("no-solution", "(120){4}1/4/7,E"),
        ("invalid", "(120){4}1-2[4:1],E"),
        ("v2-home", "(120){4}1,8,2,7,3,6,4,5,E"),
        ("v2-palm", "(120){4}C/B1/E1/7,E"),
        ("v2-swap", "(120){4}1-5[4:1]/5-1[4:1],E"),
        ("v3-jack", "(240){24}1,1,1,1,E"),
        ("v3-abab", "(240){24}1,2,1,2,1,2,E"),
        ("v3-palm", "(120){4}C/B1/E1/7,E"),
        ("v3-wifi", "(120){4}1w5[4:2],{8},C,E"),
        ("v3-slide-palm", "(120){4}1-5[4:1]/8h[4:3],{16},B2,E"),
    ] {
        let mut config = SolverConfig::default();
        if name.starts_with("v2-") {
            config = SolverConfig::v2();
        }
        if name.starts_with("v3-") {
            config = SolverConfig::v3();
        }
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
            "{name}: {}, notes={}, handovers={}",
            response.status,
            response.chart.as_ref().map_or(0, |c| c.notes.len()),
            response.solutions.first().map_or(0, |s| s.handovers.len())
        );
    }
}
