//! stairChain（2026-11-23）：相接的快速階梯，每串一隻手、串與串換手，第一串由多數鍵那一側的手起手。
//! 預設 0；只有 V4.3 開（三個權重都是 1，使用者同日決定 V4.3 取代 V4.1 當 App 主卡）。
use mai_motion_core::scoring_v3::{variant_tuning, with_tuning, TuningV3, V3_VARIANTS};
use mai_motion_core::*;

/// V4.1 加上 stairChain 三個權重之後，每顆音符第一次接觸的手，依時間。
fn hands(source: &str, keep: f64, alt: f64, side: f64) -> Vec<Hand> {
    let tuning: &'static TuningV3 = Box::leak(Box::new(TuningV3 {
        stair_chain: keep,
        stair_chain_alt: alt,
        stair_chain_side: side,
        ..variant_tuning("v4.1").unwrap().clone()
    }));
    let chart = parse_chart(source, 0.0).unwrap().chart;
    let response = with_tuning(tuning, || {
        analyze_chart(AnalyzeRequest {
            request_id: "t".into(),
            source: source.into(),
            first_seconds: 0.0,
            solver_config: SolverConfig::v3(),
        })
    });
    assert_eq!(response.status, "ok", "{:?}", response.diagnostics);
    let solution = response.solutions.into_iter().next().unwrap();
    let mut notes: Vec<&Note> = chart.notes.iter().collect();
    notes.sort_by(|a, b| a.time_seconds.total_cmp(&b.time_seconds));
    notes
        .iter()
        .map(|n| {
            solution
                .assignments
                .iter()
                .find(|a| a.note_id == n.id && matches!(a.part.as_str(), "contact" | "head"))
                .unwrap()
                .hand
        })
        .collect()
}

#[test]
fn stair_chain_only_in_v43() {
    let t = TuningV3::default();
    assert_eq!((t.stair_chain, t.stair_chain_alt, t.stair_chain_side), (0.0, 0.0, 0.0));
    for variant in V3_VARIANTS {
        let t = variant_tuning(variant).unwrap();
        let w = if variant == "v4.3" { 1.0 } else { 0.0 };
        assert_eq!(
            (t.stair_chain, t.stair_chain_alt, t.stair_chain_side),
            (w, w, w),
            "{}",
            variant
        );
    }
}

/// V4.3 和 V4.1 只差三條節奏規則（stairChain、jackExit、jackAlt）。
#[test]
fn v43_is_v41_plus_three_rules() {
    let v43 = variant_tuning("v4.3").unwrap();
    let back = TuningV3 {
        stair_chain: 0.0,
        stair_chain_alt: 0.0,
        stair_chain_side: 0.0,
        jack_exit: 0.0,
        jack_alt: 0.0,
        ..v43.clone()
    };
    assert_eq!(
        serde_json::to_value(&back).unwrap(),
        serde_json::to_value(variant_tuning("v4.1").unwrap()).unwrap()
    );
    assert_eq!((v43.jack_exit, v43.jack_alt), (0.3, 0.5));
}

/// Re:Unknown X 113 秒那種 16 分的階梯一串接一串（8765｜8765｜187654）：每串一隻手、
/// 串與串換手，第一串在左半所以左手起手。V4.1 會把一串的最後一顆交給另一隻手。
#[test]
fn chained_stairs_alternate_run_by_run() {
    let src = "(190){16}8,7,6,5,8,7,6,5,1,8,7,6,5,4,,,,,E";
    let h = hands(src, 1.0, 1.0, 1.0);
    let runs = [&h[0..4], &h[4..8], &h[8..14]];
    for run in runs {
        assert!(run.iter().all(|x| *x == run[0]), "{h:?}");
    }
    assert_eq!(runs[0][0], Hand::L, "{h:?}");
    assert_ne!(runs[0][0], runs[1][0], "{h:?}");
    assert_ne!(runs[1][0], runs[2][0], "{h:?}");
}

/// 單獨一串（前後沒有相接的串）不算，權重再大也和 V4.1 一樣。
#[test]
fn a_lone_stair_is_untouched() {
    let src = "(190){16}8,7,6,5,,,,,{4}2,,E";
    assert_eq!(hands(src, 5.0, 5.0, 5.0), hands(src, 0.0, 0.0, 0.0));
}
