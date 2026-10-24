//! 備用手掌半徑：平常用 palmRadius，整首走不通時才改用較大的 palmFallbackRadius 重試。
use mai_motion_core::*;

/// 開頭 12 毫秒內兩顆星星加 8 個 Touch（以 {512} 寫成的拍打），手掌半徑 0.35 時兩手蓋不完。
const SLAM: &str = "(191)\n{512}1x/8x<1[4:11]*q1[4:11],B8/B1,C,B3/B6,B4/B5,E5,,,,,,,,,,,{32},,,{8},{1},,,{4},1s5[8:1]/8z4[8:1],,\n{8}2h[8:3],,,4h[8:3],,,{2}5p4[4:3],{8},7,,8,8,,7h[8:3],,,5h[8:3],,,{2}4q5[4:3],{8},2,,1,1,,\nE";

fn analyze(palm: f64, fallback: f64) -> AnalyzeResponse {
    let mut solver_config = SolverConfig::v3();
    solver_config.palm_radius = palm;
    solver_config.palm_fallback_radius = fallback;
    analyze_chart(AnalyzeRequest {
        request_id: "palm".into(),
        source: SLAM.into(),
        first_seconds: 0.0,
        solver_config,
    })
}

#[test]
fn small_palm_alone_has_no_solution() {
    let response = analyze(0.35, 0.0);
    assert_eq!(response.status, "no_solution");
}

#[test]
fn fallback_palm_is_used_only_when_needed_and_noted() {
    let response = analyze(0.35, 0.7);
    assert_eq!(response.status, "ok", "{:?}", response.diagnostics);
    let top = &response.solutions[0];
    assert!(
        top.warnings.iter().any(|w| w.contains("備用手掌半徑 0.7")),
        "{:?}",
        top.warnings
    );
    assert_eq!(top.config_snapshot.palm_radius, 0.7);

    // 平常的半徑就走得通時不用備用半徑
    let response = analyze(0.5, 0.7);
    assert_eq!(response.status, "ok");
    let top = &response.solutions[0];
    assert!(!top.warnings.iter().any(|w| w.contains("備用手掌半徑")));
    assert_eq!(top.config_snapshot.palm_radius, 0.5);
}

#[test]
fn fallback_radius_is_validated_and_defaults_for_old_configs() {
    let mut c = SolverConfig::v3();
    c.palm_fallback_radius = 1.5;
    assert!(c.validate().is_err());
    let old: SolverConfig =
        serde_json::from_str(r#"{"scoringModel":"human-motion-v3","palmRadius":0.4}"#).unwrap();
    assert_eq!(old.palm_fallback_radius, 0.7);
}
