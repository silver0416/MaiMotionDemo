//! sideRest：兩手都空著（0.26 秒以上沒有接觸、長押或滑行）時，用手打另一半的單顆收成本。
//! 預設 0（V3、V3.1、V3.2、V4 結果不變）；V4.1 開 0.2（2026-10-31）。
use mai_motion_core::scoring_v3::{variant_tuning, TuningV3};
use mai_motion_core::*;

/// 每顆音符第一次接觸的手，依 key。
fn hands(source: &str, variant: &str) -> Vec<(String, Hand)> {
    let chart = parse_chart(source, 0.0).unwrap().chart;
    let mut solver_config = SolverConfig::v3();
    solver_config.v3_variant = Some(variant.into());
    let response = analyze_chart(AnalyzeRequest {
        request_id: "t".into(),
        source: source.into(),
        first_seconds: 0.0,
        solver_config,
    });
    assert_eq!(response.status, "ok", "{:?}", response.diagnostics);
    let solution = &response.solutions[0];
    chart
        .notes
        .iter()
        .map(|n| {
            let hand = solution
                .assignments
                .iter()
                .find(|a| a.note_id == n.id && matches!(a.part.as_str(), "contact" | "head"))
                .unwrap()
                .hand;
            (n.key.clone(), hand)
        })
        .collect()
}

#[test]
fn side_rest_only_in_v41() {
    assert_eq!(TuningV3::default().side_rest, 0.0);
    for id in ["v3.1", "v3.2", "v4"] {
        assert_eq!(variant_tuning(id).unwrap().side_rest, 0.0, "{id}");
    }
    let v41 = variant_tuning("v4.1").unwrap();
    assert_eq!((v41.side_rest, v41.side_rest_window), (0.2, 0.26));
    // V4.1 和 V4 只差 sideRest 與 holdCross（2026-11-02 併入，見 tests/unit_rules.rs）
    let (mut a, mut b) = (
        serde_json::to_value(variant_tuning("v4").unwrap()).unwrap(),
        serde_json::to_value(v41).unwrap(),
    );
    for v in [&mut a, &mut b] {
        v.as_object_mut().unwrap().remove("sideRest");
        v.as_object_mut().unwrap().remove("holdCross");
    }
    assert_eq!(a, b, "V4.1 和 V4 只差 sideRest 與 holdCross");
}

/// AMABIE（DX MASTER）開頭：每拍一顆、中間夾 16 分的軸心顫音（6-5-6-7-6），軸心 6 在左半。
/// 你的標記軸心一律左手；V4 讓右手過來打 6，V4.1 交給左手、5 和 7 由右手補。
#[test]
fn side_rest_keeps_pivot_on_its_half() {
    let src = "(190){16}3,,,,2,,,,5,,,,6,,,,6,5,6,7,6,,,,6,,,,7,,,,4,,,,E";
    let sixes = |hands: &[(String, Hand)]| {
        hands
            .iter()
            .filter(|(key, _)| key.ends_with("|tap|6"))
            .map(|(_, hand)| *hand)
            .collect::<Vec<_>>()
    };
    assert!(sixes(&hands(src, "v4")).iter().all(|h| *h == Hand::R));
    let v41 = hands(src, "v4.1");
    assert_eq!(sixes(&v41), [Hand::L; 5]);
    let at = |key: &str| v41.iter().find(|(k, _)| k == key).unwrap().1;
    assert_eq!((at("1.342|tap|5"), at("1.500|tap|7")), (Hand::R, Hand::R));
}
