//! jackAlt：同一鍵的快速單顆連打（每下不到 0.13 秒），四下以上或每下都不到 0.08 秒時，
//! 相鄰兩下同一隻手收成本（標記者這種串一律兩手交互）。預設 0，所有內建版本都不變。
use mai_motion_core::scoring_v3::{variant_tuning, with_tuning, TuningV3, V3_VARIANTS};
use mai_motion_core::*;

/// V4.1 加上 jackAlt 之後，每顆音符第一次接觸的手，依時間。
fn hands(source: &str, weight: f64) -> Vec<Hand> {
    let chart = parse_chart(source, 0.0).unwrap().chart;
    let tuning: &'static TuningV3 = Box::leak(Box::new(TuningV3 {
        jack_alt: weight,
        ..variant_tuning("v4.1").unwrap().clone()
    }));
    let response = with_tuning(tuning, || {
        analyze_chart(AnalyzeRequest {
            request_id: "t".into(),
            source: source.into(),
            first_seconds: 0.0,
            solver_config: SolverConfig::v3(),
        })
    });
    assert_eq!(response.status, "ok", "{:?}", response.diagnostics);
    let solution = &response.solutions[0];
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

fn alternates(hands: &[Hand]) -> bool {
    hands.windows(2).all(|p| p[0] != p[1])
}

#[test]
fn jack_alt_off_everywhere() {
    assert_eq!(TuningV3::default().jack_alt, 0.0);
    for id in V3_VARIANTS {
        assert_eq!(variant_tuning(id).unwrap().jack_alt, 0.0, "{id}");
    }
}

/// エータ・ベータ・イータ 142.3 秒那種 16 分五連打：V4.1 一隻手打完，開 jackAlt 後交互。
#[test]
fn long_fast_jack_alternates() {
    let src = "(186){16}6,6,6,6,6,,,,E";
    assert!(hands(src, 0.0).windows(2).all(|p| p[0] == p[1]));
    assert!(alternates(&hands(src, 1.0)));
    // 夾在其他音符中間的四連打也交互
    let src = "(150){16}3,,,,2,2,2,2,1,,,,E";
    assert!(alternates(&hands(src, 1.0)[1..5]));
}

/// 較慢的兩下連打（16 分、0.107 秒）不算，留在同一隻手。
#[test]
fn slow_double_stays_with_one_hand() {
    let src = "(140){16}1,1,,,,,,,8,8,,,,,,,E";
    assert_eq!(hands(src, 1.0), hands(src, 0.0));
    let h = hands(src, 1.0);
    assert!(h[0] == h[1] && h[2] == h[3]);
}
