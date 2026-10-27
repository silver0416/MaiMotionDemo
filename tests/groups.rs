//! 分組：給定分組（研究用）與自動分組（預設開啟）的組內手型成本、自動分組的切法、拍點位置。
use mai_motion_core::scoring_v3::{with_tuning, TuningV3};
use mai_motion_core::*;

/// 16 分連打 6 5 8 1 4 3 7 1，接 24 分階梯 7 6 5、1 2 3。
const STREAM: &str = "(155){16}6,5,8,1,4,3,7,1,{24}7,6,5,1,2,3,E";

fn request(source: &str) -> AnalyzeRequest {
    AnalyzeRequest {
        request_id: "g".into(),
        source: source.into(),
        first_seconds: 0.0,
        solver_config: SolverConfig::v3(),
    }
}

/// 每顆音符第一次接觸的手（依 chart.notes 順序）。
fn hands(chart: &Chart, response: &AnalyzeResponse) -> Vec<Hand> {
    let solution = &response.solutions[0];
    chart
        .notes
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

fn solve(source: &str, groups: Option<Vec<Vec<usize>>>) -> (Chart, Vec<Hand>) {
    let chart = parse_chart(source, 0.0).unwrap().chart;
    let response = match groups {
        Some(groups) => {
            let keys = groups
                .iter()
                .map(|g| g.iter().map(|i| chart.notes[*i].key.clone()).collect())
                .collect();
            with_groups(keys, || analyze_chart(request(source)))
        }
        None => analyze_chart(request(source)),
    };
    let hands = hands(&chart, &response);
    (chart, hands)
}

/// 關掉自動分組（groupAuto 0）跑 `f`，當作沒有分組成本的對照。
fn without_auto<R>(f: impl FnOnce() -> R) -> R {
    let tuning: &'static TuningV3 = Box::leak(Box::new(TuningV3 {
        group_auto: 0.0,
        ..TuningV3::default()
    }));
    with_tuning(tuning, f)
}

/// 自動分組，每組以 chart.notes 的索引表示，組與組內依索引排序。
fn auto(source: &str) -> Vec<Vec<usize>> {
    indices(source, auto_group_keys)
}

/// app 標註預填用的擬人切法分組（不看 V3 權重），表示法同 [`auto`]。
fn motion(source: &str) -> Vec<Vec<usize>> {
    indices(source, motion_group_keys)
}

fn indices(source: &str, keys: fn(&Chart) -> Vec<Vec<String>>) -> Vec<Vec<usize>> {
    let chart = parse_chart(source, 0.0).unwrap().chart;
    let mut groups: Vec<Vec<usize>> = keys(&chart)
        .iter()
        .map(|g| {
            let mut g: Vec<usize> = g
                .iter()
                .map(|k| chart.notes.iter().position(|n| &n.key == k).unwrap())
                .collect();
            g.sort();
            g
        })
        .collect();
    groups.sort();
    groups
}

fn pairs() -> Vec<Vec<usize>> {
    vec![vec![0, 1], vec![2, 3], vec![4, 5], vec![6, 7]]
}

fn stairs() -> Vec<Vec<usize>> {
    vec![vec![8, 9, 10], vec![11, 12, 13]]
}

/// 沒照規則的組數：兩顆同手的、階梯沒有單手的。
fn broken(hands: &[Hand]) -> usize {
    let pairs = pairs()
        .iter()
        .filter(|p| hands[p[0]] == hands[p[1]])
        .count();
    let stairs = stairs()
        .iter()
        .filter(|s| s.iter().any(|i| hands[*i] != hands[s[0]]))
        .count();
    pairs + stairs
}

fn assert_rules(hands: &[Hand]) {
    for pair in pairs() {
        assert_ne!(hands[pair[0]], hands[pair[1]], "{pair:?} {hands:?}");
    }
    for stair in stairs() {
        assert!(
            stair.iter().all(|i| hands[*i] == hands[stair[0]]),
            "{stair:?} {hands:?}"
        );
    }
}

/// 16 分連打兩顆一組時每組一左一右；快速三連階梯一組時單手掃過。
#[test]
fn given_groups_follow_the_hand_rules() {
    let (_, plain) = without_auto(|| solve(STREAM, None));
    let (chart, hands) = solve(STREAM, Some(pairs().into_iter().chain(stairs()).collect()));
    assert_eq!(chart.notes.len(), 14);
    assert_rules(&hands);
    // 沒給分組的模型至少有一組沒照規則，確認測試真的量到分組的作用。
    assert!(broken(&plain) > 0, "{plain:?}");
}

/// with_groups 結束後恢復原狀：同一執行緒再解一次，與沒給分組時相同。
#[test]
fn given_groups_do_not_leak_past_the_call() {
    let source = "(155){16}6,5,8,1,4,3,7,1,E";
    let (_, before) = solve(source, None);
    let _ = solve(source, Some(vec![vec![0, 1], vec![2, 3]]));
    let (_, after) = solve(source, None);
    assert_eq!(before, after);
}

/// 拍點依當時的 BPM 累計：120 BPM 兩拍後換 60 BPM。
#[test]
fn notes_carry_their_beat_position() {
    let chart = parse_chart("(120){4}1,2,(60){4}3,4,E", 0.0).unwrap().chart;
    let beats: Vec<f64> = chart.notes.iter().map(|n| n.beat).collect();
    assert_eq!(beats, vec![0.0, 1.0, 2.0, 3.0]);
}

/// 階梯自成一組；均勻的 16 分連打從段首起兩顆一組。
#[test]
fn auto_groups_cut_streams_by_beat_and_keep_stairs() {
    let mut expected: Vec<Vec<usize>> = pairs().into_iter().chain(stairs()).collect();
    expected.sort();
    assert_eq!(auto(STREAM), expected);
}

/// 節奏：前後都比較稀的三連自成一組（LINK 的三角）；同鍵反覆連成一組，Hold 不接上。
#[test]
fn auto_groups_follow_rhythm_and_repeated_keys() {
    // 8, 1/7 每拍一個，接 6 5 7 半拍一個，再每拍一個。
    let groups = auto("(198){4}8,1/7,{8}6,5,7,,{4}1,2/8,E");
    assert!(groups.contains(&vec![3, 4, 5]), "{groups:?}");
    // 同一顆鍵 8 分連打 5 下，最後一下是 Hold。
    let groups = auto("(150){8}8,8,8,8,8h[8:1],E");
    assert_eq!(groups, vec![vec![0, 1, 2, 3]]);
}

/// 用擬人切法（groupCut 1）跑 `f`。
fn motion_cut<R>(f: impl FnOnce() -> R) -> R {
    let tuning: &'static TuningV3 = Box::leak(Box::new(TuningV3 {
        group_cut: 1.0,
        ..TuningV3::default()
    }));
    with_tuning(tuning, f)
}

/// 擬人切法（預設關）：均勻的 16 分連打在手跳得比前後都遠的地方切，不照拍點兩顆一組。
/// 1 3 2 4 5 8 7 2：2→4、5→8 是比前後都遠的跳躍。
#[test]
fn motion_cut_splits_streams_where_the_hand_jumps() {
    assert_eq!(TuningV3::default().group_cut, 0.0);
    let source = "(160){16}1,3,2,4,5,8,7,2,E";
    assert_eq!(
        auto(source),
        vec![vec![0, 1], vec![2, 3], vec![4, 5], vec![6, 7]]
    );
    assert_eq!(
        motion_cut(|| auto(source)),
        vec![vec![0, 1, 2], vec![3, 4], vec![5, 6, 7]]
    );
    // app 預填用的不看權重：預設權重下也是擬人切法。
    assert_eq!(motion(source), motion_cut(|| auto(source)));
}

/// 擬人切法：接 Hold 的那顆 Tap 和 Hold 一組（點完接著按是一個動作）。
#[test]
fn motion_cut_keeps_a_tap_with_the_hold_it_leads_into() {
    // 同一顆鍵 8 分連打 4 下接 Hold：拍點切法反覆一路到 Hold 前，擬人切法最後一下留給 Hold。
    let source = "(150){8}8,8,8,8,8h[8:1],E";
    assert_eq!(auto(source), vec![vec![0, 1, 2, 3]]);
    assert_eq!(motion_cut(|| auto(source)), vec![vec![0, 1, 2], vec![3, 4]]);
    // 8 分 1 4 6 接 5 的 Hold：四顆一段時拍點切法不切；擬人切法 6 跟著 Hold。
    let source = "(150){8}1,4,6,5h[8:1],E";
    assert_eq!(auto(source), vec![vec![0, 1, 2, 3]]);
    assert_eq!(motion_cut(|| auto(source)), vec![vec![0, 1], vec![2, 3]]);
}

/// 擬人切法：留給 Hold 的那顆沒和 Hold 分在一起時還給階梯，不會落單（沒有分組）。
#[test]
fn motion_cut_returns_the_tap_when_the_hold_starts_its_own_run() {
    // 1 2 3 4 階梯接 5 的 Hold，Hold 又開始 5 6 7 階梯：4 回到前一組。
    let source = "(150){16}1,2,3,4,5h[16:1],6,7,E";
    assert_eq!(auto(source), vec![vec![0, 1, 2, 3], vec![4, 5, 6]]);
    assert_eq!(
        motion_cut(|| auto(source)),
        vec![vec![0, 1, 2, 3], vec![4, 5, 6]]
    );
    assert_eq!(motion(source), vec![vec![0, 1, 2, 3], vec![4, 5, 6]]);
}

/// 擬人切法：同鍵反覆後落單的同鍵 Tap 只在間隔夠短時併進反覆，隔了休息就不併。
#[test]
fn motion_cut_attaches_a_trailing_jack_tap_only_without_a_rest() {
    // 16 分連打三下，空一拍再一下：最後一下併進反覆。
    let source = "(150){16}8,8,8,,8,E";
    assert_eq!(auto(source), vec![vec![0, 1, 2]]);
    assert_eq!(motion(source), vec![vec![0, 1, 2, 3]]);
    // 8 分連打三下，休息 2 秒後在譜尾再一下：不併。
    let source = "(150){8}8,8,8,,,,,,,,,,8,E";
    assert_eq!(motion(source), vec![vec![0, 1, 2]]);
    // 譜中間：休息 1 秒後一下、再休息更久：不併。
    let source = "(150){8}8,8,8,,,,,8,,,,,,,,,1,E";
    assert_eq!(motion(source), vec![vec![0, 1, 2]]);
}

/// 快速同鍵連打（16 分、六顆）一路交替，起手接續前面 2／8 交替的下一手；關掉自動分組時不會。
#[test]
fn fast_jacks_alternate_from_the_previous_hand() {
    let source = "(185){16}2,8,1,1,1,1,1,1,E";
    let chart = parse_chart(source, 0.0).unwrap().chart;
    let plain = without_auto(|| hands(&chart, &analyze_chart(request(source))));
    let grouped = hands(&chart, &analyze_chart(request(source)));
    let alternates = |h: &[Hand]| h[1..].windows(2).all(|w| w[0] != w[1]);
    assert!(alternates(&grouped), "{grouped:?}");
    assert!(!alternates(&plain), "{plain:?}");
}

/// 自動分組預設開啟（groupAuto 0.5）。規則（16 分相鄰兩顆一左一右、快速短階梯單手）在權重夠大時全部照做；
/// 0.5 只把差不多的選擇往規則推，這段連打在 0.5 時不變（預設的作用由快速同鍵連打的測試量）。
#[test]
fn auto_groups_are_on_by_default() {
    assert_eq!(TuningV3::default().group_auto, 0.5);
    let chart = parse_chart(STREAM, 0.0).unwrap().chart;
    let solve_with = |weight: f64| {
        let tuning: &'static TuningV3 = Box::leak(Box::new(TuningV3 {
            group_auto: weight,
            ..TuningV3::default()
        }));
        with_tuning(tuning, || hands(&chart, &analyze_chart(request(STREAM))))
    };
    let plain = solve_with(0.0);
    assert!(broken(&plain) > 0, "{plain:?}");
    assert_rules(&solve_with(4.0));
}
