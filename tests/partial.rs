//! 部分解：模型本身走不下去時略過做不到的音符，其餘照常求解（analyze_chart_partial）。
use mai_motion_core::*;

fn request(source: &str, solver_config: SolverConfig) -> AnalyzeRequest {
    AnalyzeRequest {
        request_id: "partial".into(),
        source: source.into(),
        first_seconds: 0.0,
        solver_config,
    }
}

fn note_id(source: &str, key: &str) -> String {
    let chart = parse_chart(source, 0.0).unwrap().chart;
    chart
        .notes
        .iter()
        .find(|n| n.key == key)
        .unwrap()
        .id
        .clone()
}

/// 兩手都按著 Hold 時來的 Tap：一般分析回報無解；部分解只略過這顆 Tap，兩個 Hold 照常分手。
#[test]
fn only_the_note_that_cannot_be_played_is_skipped() {
    let source = "(120){4}1h[1:1]/7h[1:1],,5,,E";
    for config in [SolverConfig::default(), SolverConfig::v3()] {
        assert_eq!(
            analyze_chart(request(source, config.clone())).status,
            "no_solution"
        );
        let r = analyze_chart_partial(request(source, config));
        assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
        let tap = note_id(source, "1.000|tap|5");
        for s in &r.solutions {
            assert_eq!(
                s.skipped,
                [SkippedPart {
                    note_id: tap.clone(),
                    part: "contact".into(),
                    time_seconds: 1.0,
                }]
            );
            assert!(s.assignments.iter().all(|a| a.note_id != tap));
            let hands: Vec<Hand> = ["0.000|hold|1", "0.000|hold|7"]
                .iter()
                .map(|key| {
                    let id = note_id(source, key);
                    s.assignments.iter().find(|a| a.note_id == id).unwrap().hand
                })
                .collect();
            assert_ne!(hands[0], hands[1]);
            assert!(s.warnings.iter().any(|w| w.contains("模型有 1 處走不下去")));
        }
    }
}

/// 整首解得出來時部分解與一般分析完全相同，輸出也沒有 skipped 欄位。
#[test]
fn a_playable_chart_is_unchanged() {
    let source = "(150){8}1,2,3,4,5/1,6,7-3[4:1],8,{4}A1/B2,Ch[4:1],2,E";
    let full = analyze_chart(request(source, SolverConfig::v3()));
    let partial = analyze_chart_partial(request(source, SolverConfig::v3()));
    assert_eq!(full.status, "ok");
    let json = |r: &AnalyzeResponse| serde_json::to_string(&r.solutions).unwrap();
    assert_eq!(json(&full), json(&partial));
    assert!(!json(&partial).contains("\"skipped\""));
}

/// 星星與它的滑行分開略過：兩手都被 Hold 佔住時，星星頭與滑行都做不到；Hold 放開後的 Tap 照打。
#[test]
fn a_star_and_its_slide_are_skipped_separately() {
    let source = "(120){4}1h[1:1]/5h[1:1],3-7[4:1],,,2,E";
    let r = analyze_chart_partial(request(source, SolverConfig::v3()));
    assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
    let star = note_id(source, "0.500|slide|3-7");
    let s = &r.solutions[0];
    let parts: Vec<(&str, &str)> = s
        .skipped
        .iter()
        .map(|p| (p.note_id.as_str(), p.part.as_str()))
        .collect();
    assert_eq!(
        parts,
        [(star.as_str(), "contact"), (star.as_str(), "slide")],
        "{:?}",
        s.skipped
    );
    assert!(s.assignments.iter().all(|a| a.note_id != star));
    let tap = note_id(source, "2.000|tap|2");
    assert!(s.assignments.iter().any(|a| a.note_id == tap));
}

/// Temoto：基座略過的音符，照修正後的手重解時一樣略過，結果照列。
#[test]
fn temoto_keeps_the_base_skips() {
    let source = "(120){4}1h[1:1]/7h[1:1],,5,,3,6,E";
    let mut config = SolverConfig::v3();
    config.ml_model = Some("temoto-1-konruri".into());
    assert_eq!(
        analyze_chart(request(source, config.clone())).status,
        "no_solution"
    );
    let r = analyze_chart_partial(request(source, config));
    assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
    let tap = note_id(source, "1.000|tap|5");
    let s = &r.solutions[0];
    assert_eq!(s.skipped.len(), 1);
    assert_eq!(s.skipped[0].note_id, tap);
    assert!(s.warnings[0].starts_with("Temoto-1"), "{:?}", s.warnings);
    assert!(s.warnings.iter().any(|w| w.contains("模型有 1 處走不下去")));
    for key in ["2.000|tap|3", "2.500|tap|6"] {
        let id = note_id(source, key);
        assert!(s.assignments.iter().any(|a| a.note_id == id), "{key}");
    }
}
