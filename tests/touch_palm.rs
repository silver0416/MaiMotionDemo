//! touchPalm：一隻手掌同時蓋住太散的 Touch 要另外付成本（標記者在包圍圓半徑約 0.35 以內
//! 才一手包，0.4 以上幾乎都兩手分）。預設 0（V3 系列結果不變），V4 開 0.25。
use mai_motion_core::scoring_v3::{variant_tuning, with_tuning, TuningV3};
use mai_motion_core::*;

/// 每顆 Touch 接觸的手，依 key。
fn hands(source: &str, weight: f64) -> Vec<(String, Hand)> {
    let chart = parse_chart(source, 0.0).unwrap().chart;
    let tuning: &'static TuningV3 = Box::leak(Box::new(TuningV3 {
        touch_palm: weight,
        ..variant_tuning("v4").unwrap().clone()
    }));
    let response = with_tuning(tuning, || {
        analyze_chart(AnalyzeRequest {
            request_id: "t".into(),
            source: source.into(),
            first_seconds: 0.0,
            solver_config: SolverConfig::v3(),
        })
    });
    let solution = &response.solutions[0];
    chart
        .notes
        .iter()
        .map(|n| {
            let hand = solution
                .assignments
                .iter()
                .find(|a| a.note_id == n.id && a.part == "contact")
                .unwrap()
                .hand;
            (n.key.clone(), hand)
        })
        .collect()
}

fn both_hands(hands: &[(String, Hand)]) -> bool {
    hands.iter().any(|(_, h)| *h == Hand::L) && hands.iter().any(|(_, h)| *h == Hand::R)
}

#[test]
fn touch_palm_only_in_v4() {
    assert_eq!(TuningV3::default().touch_palm, 0.0);
    for id in ["v3.1", "v3.2"] {
        assert_eq!(variant_tuning(id).unwrap().touch_palm, 0.0, "{id}");
    }
    assert_eq!(variant_tuning("v4").unwrap().touch_palm, 0.25);
}

fn hand_of(hands: &[(String, Hand)], area: &str) -> Hand {
    hands
        .iter()
        .find(|(key, _)| key.ends_with(&format!("|{area}")))
        .unwrap()
        .1
}

/// 下半圈五顆：沒開時一隻手掌蓋住 B6～B3（包圍圓半徑約 0.43）；開了以後兩端分給
/// 兩隻手，左手拿左邊的 B6、右手拿右邊的 B3。
#[test]
fn wide_chord_is_split() {
    const SRC: &str = "(120){4}B6/B5/E5/B4/B3,E";
    let plain = hands(SRC, 0.0);
    assert_eq!(hand_of(&plain, "B6"), hand_of(&plain, "B3"), "{plain:?}");
    let ruled = hands(SRC, 4.0);
    assert_eq!(
        (hand_of(&ruled, "B6"), hand_of(&ruled, "B3")),
        (Hand::L, Hand::R),
        "{ruled:?}"
    );
}

/// 上方三顆靠在一起（半徑約 0.25）：開了也還是一手包。
#[test]
fn tight_chord_stays_one_palm() {
    const SRC: &str = "(120){4}E8/B8/E1,E";
    let ruled = hands(SRC, 4.0);
    assert!(!both_hands(&ruled), "{ruled:?}");
}
