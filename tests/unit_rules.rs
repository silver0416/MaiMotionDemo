//! 標記者回答（2026-11-01 起手手順問答）整理出的三條規則：
//! - stairUnit：快速階梯四顆一組（或前後以跳鍵分開的三顆）一隻手刷完；
//! - jackExit：同鍵兩顆一隻手打完，下一顆換鍵就換手；
//! - holdCross：長押開始的和弦，長押給那一側的手，不交叉、也不一手包下散開的和弦。
//!
//! 預設都是 0。holdCross 在 2026-11-02 併進 V4.1（使用者決定；權重 5 依開發集選，
//! 1／2–3／5–10 為 +120／+123／+153），其他版本不變；
//! stairUnit、jackExit 在 eval18 不穩，所有內建版本都不開。
use mai_motion_core::scoring_v3::{variant_tuning, with_tuning, TuningV3, V3_VARIANTS};
use mai_motion_core::*;

/// V4.1 加上指定權重之後，每顆音符第一次接觸的手，依時間（同時刻依按鍵）。
fn hands(source: &str, stair_unit: f64, jack_exit: f64) -> Vec<Hand> {
    hands_with(
        source,
        TuningV3 {
            stair_unit,
            jack_exit,
            ..variant_tuning("v4.1").unwrap().clone()
        },
    )
}

/// 指定的 note key（時間|種類|鍵）第一次接觸的手。
fn hand_of(source: &str, tuning: TuningV3, key: &str) -> Hand {
    let (chart, solution) = solve(source, tuning);
    let id = &chart
        .notes
        .iter()
        .find(|n| n.key.ends_with(key))
        .unwrap()
        .id;
    first_hand(&solution, id)
}

fn first_hand(solution: &Solution, id: &str) -> Hand {
    solution
        .assignments
        .iter()
        .find(|a| a.note_id == id && matches!(a.part.as_str(), "contact" | "head"))
        .unwrap()
        .hand
}

fn solve(source: &str, tuning: TuningV3) -> (Chart, Solution) {
    let chart = parse_chart(source, 0.0).unwrap().chart;
    let tuning: &'static TuningV3 = Box::leak(Box::new(tuning));
    let response = with_tuning(tuning, || {
        analyze_chart(AnalyzeRequest {
            request_id: "t".into(),
            source: source.into(),
            first_seconds: 0.0,
            solver_config: SolverConfig::v3(),
        })
    });
    assert_eq!(response.status, "ok", "{:?}", response.diagnostics);
    (chart, response.solutions.into_iter().next().unwrap())
}

fn hands_with(source: &str, tuning: TuningV3) -> Vec<Hand> {
    let (chart, solution) = solve(source, tuning);
    let mut notes: Vec<&Note> = chart.notes.iter().collect();
    notes.sort_by(|a, b| {
        a.time_seconds
            .total_cmp(&b.time_seconds)
            .then(a.button.cmp(&b.button))
    });
    notes.iter().map(|n| first_hand(&solution, &n.id)).collect()
}

#[test]
fn unit_rules_only_hold_cross_in_v41() {
    let t = TuningV3::default();
    assert_eq!((t.stair_unit, t.jack_exit, t.hold_cross), (0.0, 0.0, 0.0));
    for id in V3_VARIANTS {
        let t = variant_tuning(id).unwrap();
        let hold_cross = if id == "v4.1" { 5.0 } else { 0.0 };
        assert_eq!(
            (t.stair_unit, t.jack_exit, t.hold_cross, t.hold_cross_min),
            (0.0, 0.0, hold_cross, 0.3),
            "{id}"
        );
    }
}

fn hold_cross(weight: f64) -> TuningV3 {
    TuningV3 {
        hold_cross: weight,
        ..variant_tuning("v4.1").unwrap().clone()
    }
}

/// TECHNOPOLIS 2085（DX MASTER）原譜片段，前面接一段和弦，後面接長押下的交替。
const TECHNOPOLIS_59: &str = "(134){16}1/7,,2/8,2/8,,4/5,4/5,,3-6[8:1]/7-2[8:1],,,,3b/7b,,,,
    {8}E5h[8:23]/7>2[8:1],,,,2h[8:1],B5,7h[8:1],B4,
    2h[8:1],B5,7h[8:1],B4,2,1/B5,8,1/B4,
    2h[8:1],B5,7h[8:1],B4,2<7[8:1],B5,,B4,E";
const TECHNOPOLIS_124: &str = "(134){16}2/8,,7,7,8,,6,6,5,,3,3,4,,2/5,,
    {8}B2/B3/E5h[8:23],,8h[8:1],,2h[8:1],B5,8h[8:1],B4,
    2h[8:1],B5,8h[8:1],B4,1h[8:1],B5,7h[8:1],B4,E";

/// 59.1 秒：左邊的星星和下面的 Touch 長押同時。標記是星星左手、長押右手；併入前的 V4.1 交叉。
#[test]
fn hold_chord_keeps_sides() {
    let at = |w: f64, key: &str| hand_of(TECHNOPOLIS_59, hold_cross(w), key);
    assert_eq!(
        (at(0.0, "|slide|7>2"), at(0.0, "|touchHold|E5")),
        (Hand::R, Hand::L)
    );
    assert_eq!(
        (at(5.0, "|slide|7>2"), at(5.0, "|touchHold|E5")),
        (Hand::L, Hand::R)
    );
}

/// 124.5 秒：Touch 長押和右邊散開的兩顆 Touch 同時。標記是長押左手、B2／B3 右手。
#[test]
fn hold_chord_not_one_spread_palm() {
    let at = |w: f64, key: &str| hand_of(TECHNOPOLIS_124, hold_cross(w), key);
    let chord = |w: f64| {
        (
            at(w, "|touchHold|E5"),
            at(w, "|touch|B2"),
            at(w, "|touch|B3"),
        )
    };
    assert_ne!(chord(0.0), (Hand::L, Hand::R, Hand::R));
    assert_eq!(chord(5.0), (Hand::L, Hand::R, Hand::R));
}

/// Ai C 39 秒那種 16 分四顆一組的階梯：每組一隻手，組與組換手。
#[test]
fn stair_units_stay_in_one_hand() {
    let src = "(191){16}2,3,4,5,8,7,6,5,7,6,5,4,1,2,3,4,,,,,E";
    let h = hands(src, 1.0, 0.0);
    for unit in h.chunks(4) {
        assert!(unit.iter().all(|x| *x == unit[0]), "{h:?}");
    }
    for pair in h.chunks(4).collect::<Vec<_>>().windows(2) {
        assert_ne!(pair[0][0], pair[1][0], "{h:?}");
    }
}

/// Princess♂ 36.5 秒那種 24 分 2＋1：同鍵兩顆一隻手，接著的那顆換另一隻手。
#[test]
fn jack_exit_switches_after_pair() {
    let src = "(92){24}3,3,4,2,2,4,1,1,4,8,8,4,7,7,4,6,6,4,,,,,,,E";
    let h = hands(src, 0.0, 1.0);
    for unit in h.chunks(3) {
        assert_eq!(unit[0], unit[1], "{h:?}");
        assert_ne!(unit[1], unit[2], "{h:?}");
    }
}
