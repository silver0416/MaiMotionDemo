//! V3 的兩個新成本項：極短間隔相鄰接觸換手（sweepBreak）、雙手和弦交叉（chordCross）。
//! 兩項預設 0（V3、V3.1、V3.2 結果不變），V4 才開（V4 另開 touchPalm，見 tests/touch_palm.rs）。
use mai_motion_core::scoring_v3::{variant_tuning, with_tuning, TuningV3};
use mai_motion_core::*;

fn request(source: &str) -> AnalyzeRequest {
    AnalyzeRequest {
        request_id: "s".into(),
        source: source.into(),
        first_seconds: 0.0,
        solver_config: SolverConfig::v3(),
    }
}

/// 每顆音符第一次接觸的手，依 key。
fn hands(source: &str, sweep: f64, chord: f64) -> Vec<(String, Hand)> {
    let chart = parse_chart(source, 0.0).unwrap().chart;
    let tuning: &'static TuningV3 = Box::leak(Box::new(TuningV3 {
        sweep_break: sweep,
        chord_cross: chord,
        ..variant_tuning("v3.2").unwrap().clone()
    }));
    let response = with_tuning(tuning, || analyze_chart(request(source)));
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

fn hand_of(hands: &[(String, Hand)], key: &str) -> Hand {
    hands.iter().find(|(k, _)| k == key).unwrap().1
}

#[test]
fn new_terms_default_off() {
    let t = TuningV3::default();
    assert_eq!((t.sweep_break, t.chord_cross), (0.0, 0.0));
    for id in ["v3.1", "v3.2"] {
        let t = variant_tuning(id).unwrap();
        assert_eq!((t.sweep_break, t.chord_cross), (0.0, 0.0), "{id}");
    }
}

/// V4 就是 V3.2 再打開 sweepBreak 2、chordCross 1、touchPalm 0.25（門檻 0.37），
/// 其他權重完全相同。
#[test]
fn v4_is_v32_with_new_terms() {
    let v4 = variant_tuning("v4").unwrap();
    assert_eq!((v4.sweep_break, v4.chord_cross), (2.0, 1.0));
    assert_eq!((v4.touch_palm, v4.touch_palm_free), (0.25, 0.37));
    let mut a = serde_json::to_value(v4).unwrap();
    let b = serde_json::to_value(variant_tuning("v3.2").unwrap()).unwrap();
    for k in ["sweepBreak", "chordCross", "touchPalm", "touchPalmFree"] {
        a[k] = b[k].clone();
    }
    assert_eq!(a, b, "V4 和 V3.2 只差 sweepBreak、chordCross、touchPalm");
}

/// tap 6 之後 32 分接 E6、B5（往內刷）：V3.2 讓 Touch 換給右手；開 sweepBreak 後整筆同一隻手。
#[test]
fn sweep_keeps_one_hand() {
    const SRC: &str = "(150){8}2,7,{32}6,E6,B5,{8},3,2,E";
    let keys = ["0.400|tap|6", "0.450|touch|E6", "0.500|touch|B5"];
    let plain = hands(SRC, 0.0, 0.0);
    assert_ne!(
        hand_of(&plain, keys[0]),
        hand_of(&plain, keys[1]),
        "{plain:?}"
    );
    let swept = hands(SRC, 4.0, 0.0);
    let first = hand_of(&swept, keys[0]);
    assert!(
        keys.iter().all(|k| hand_of(&swept, k) == first),
        "{swept:?}"
    );
}

/// 同側、左右位置一樣的和弦（2／3 在右側、5／8 在左側）：從另一側來幫忙的手打下面那顆。
/// V3.2 在 2／3 讓左手打上面的 2；開 chordCross 後左手打 3、5／8 由右手打 5。
#[test]
fn chord_helper_takes_lower_key() {
    const SRC: &str = "(150){4}2/3,6/7,5/8,5/8,E";
    let plain = hands(SRC, 0.0, 0.0);
    assert_eq!(hand_of(&plain, "0.000|tap|2"), Hand::L, "{plain:?}");
    let ruled = hands(SRC, 0.0, 4.0);
    for (key, hand) in [
        ("0.000|tap|2", Hand::R),
        ("0.000|tap|3", Hand::L),
        ("0.400|tap|6", Hand::R),
        ("0.400|tap|7", Hand::L),
        ("0.800|tap|5", Hand::R),
        ("0.800|tap|8", Hand::L),
    ] {
        assert_eq!(hand_of(&ruled, key), hand, "{key} {ruled:?}");
    }
}

/// 2／4 和弦：4 比較靠中線，左手打 4、右手打 2，左手不在右手右邊。
#[test]
fn chord_never_crosses() {
    const SRC: &str = "(150){4}1,2/4,7,3/4,E";
    let ruled = hands(SRC, 0.0, 4.0);
    assert_eq!(hand_of(&ruled, "0.400|tap|4"), Hand::L, "{ruled:?}");
    assert_eq!(hand_of(&ruled, "0.400|tap|2"), Hand::R, "{ruled:?}");
}
