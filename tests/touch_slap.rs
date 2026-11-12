//! 一次很多顆的同時 Touch（2026-11-12 使用者回報 Maxi 翠楼屋 13.8「搜尋達到上限」）。
//!
//! 逐顆展開時，手接得到的 Touch 一定要接，而且照固定順序指派，兩隻手常被排在前面的中央
//! 幾顆佔走；「兩掌各蓋外圈一團、中間交給 Touch Group 過半判定」展不出來，平常的手掌半徑
//! 就無解，改用備用半徑後 13 顆一組又要算好幾秒，整首算到上限。現在整組都是 Touch 時改列舉
//! 兩手的動作（大組直接這樣做，小組在逐顆展開走不下去時才這樣做）。
use mai_motion_core::*;

/// 兩手各按住一個 Hold，放開後馬上是一組同時 Touch（在 CHORD_TIME 秒）。
fn after_holds(chord: &str) -> String {
    format!("(180){{1}},\n4h[1:1],\n{{8}}2h[4:1]/6h[4:1],,,{chord},,,,,\nE")
}

/// after_holds 那組 Touch 的時間：兩小節（8 拍）再 3 個八分音符，BPM 180。
const CHORD_TIME: f64 = 19.0 / 6.0;

/// App 的預設（V4.1、快速階梯打開），手掌半徑用回報者的 0.35（App 預設 0.5 時 v1.2.0 也解得出
/// 這幾組，抓不到問題），而且不准用備用手掌半徑：平常的半徑就要解得出來。
fn analyze(source: &str) -> AnalyzeResponse {
    let mut solver_config = SolverConfig::v3();
    solver_config.v3_variant = Some("v4.1".into());
    solver_config.stair_flow = true;
    solver_config.palm_radius = 0.35;
    solver_config.palm_fallback_radius = 0.0;
    analyze_chart_partial(AnalyzeRequest {
        request_id: "slap".into(),
        source: source.into(),
        first_seconds: 0.0,
        solver_config,
    })
}

/// 這一刻每顆 Touch 實際接觸（不含過半判定連帶）的手。
fn contacts(response: &AnalyzeResponse, time: f64) -> (usize, [usize; 2]) {
    let chart = response.chart.as_ref().unwrap();
    let top = &response.solutions[0];
    let chord: Vec<&Note> = chart
        .notes
        .iter()
        .filter(|n| n.kind == "touch" && (n.time_seconds - time).abs() < 1e-6)
        .collect();
    let mut hands = [0, 0];
    for note in &chord {
        let assignment = top
            .assignments
            .iter()
            .find(|a| a.note_id == note.id)
            .unwrap_or_else(|| panic!("{} 沒有手", note.key));
        if assignment.part == "contact" {
            hands[assignment.hand.index()] += 1;
        }
    }
    (chord.len(), hands)
}

#[test]
fn thirteen_touches_are_two_palm_slaps() {
    let response = analyze(&after_holds("B1/Cf/A1/A2/D2/A3/D3/A5/A6/D6/B7/A7/D7"));
    assert_eq!(response.status, "ok", "{:?}", response.diagnostics);
    let top = &response.solutions[0];
    assert!(top.skipped.is_empty(), "{:?}", top.skipped);
    let (count, [left, right]) = contacts(&response, CHORD_TIME);
    assert_eq!(count, 13);
    // 兩隻手都拍下去，實際碰到的超過半數（其餘由過半判定連帶完成）
    assert!(left > 0 && right > 0, "左 {left}、右 {right}");
    assert!(2 * (left + right) > count, "左 {left}、右 {right}");
}

#[test]
fn small_chord_falls_back_to_two_hand_actions() {
    // 7 顆：逐顆展開會先把兩手派給 B1、C，外圈兩團都蓋不到，以前只能略過一顆
    let response = analyze(&after_holds("B1/Cf/A1/A2/D2/A6/D6"));
    assert_eq!(response.status, "ok", "{:?}", response.diagnostics);
    let top = &response.solutions[0];
    assert!(top.skipped.is_empty(), "{:?}", top.skipped);
    let (count, [left, right]) = contacts(&response, CHORD_TIME);
    assert_eq!(count, 7);
    assert!(2 * (left + right) > count, "左 {left}、右 {right}");
}

#[test]
fn maxi_intro_solves_without_fallback_palm() {
    // Maxi 開頭四組（13、10、9、5 顆），每組前面都有兩手的 Hold
    let source = "(180){1},\n4h[1:1],\n{16}5,4,5,4,5,4,5,4,5,4,5,4,3bx/5bx,,,,\n\
        {8}2h[4:1]/6h[4:1],,,B1/Cf/A1/A2/D2/A3/D3/A5/A6/D6/B7/A7/D7,,,,,\n\
        3h[4:1]/7h[4:1],,,Cf/E1/A1/B2/E2/B3/E4/B7/E7/B8,,,,,\n\
        2h[4:1]/6h[4:1],,,B1/Cf/A1/B3/A3/B5/A5/B7/A7,,,,,\n\
        1h[4:1]/7h[4:1],,,Cf/B4/A4/B8/A8,,1x,1x,1bx,\nE";
    let response = analyze(source);
    assert_eq!(response.status, "ok", "{:?}", response.diagnostics);
    let top = &response.solutions[0];
    assert!(top.skipped.is_empty(), "{:?}", top.skipped);
    for (time, count) in [(4.5, 13), (35.0 / 6.0, 10), (43.0 / 6.0, 9)] {
        let (found, [left, right]) = contacts(&response, time);
        assert_eq!(found, count, "{time}");
        assert!(2 * (left + right) > count, "{time}：左 {left}、右 {right}");
    }
}

/// 左手按住 7 的 Hold 時來一組 B2／B3／C（相鄰三顆，一個 Touch Group）。
fn held_left_then_trio() -> (String, Chart) {
    let source = "(120){4}7h[2:1],B2/B3/C,,,E".to_string();
    let chart = parse_chart(&source, 0.0).unwrap().chart;
    (source, chart)
}

#[test]
fn group_judged_touch_keeps_the_labeled_hand() {
    // 右手拍 B2、B3（過半），C 由 Touch Group 連帶判定、沒有手碰到。標註把 C 標成左手時，
    // 記錄要照標註的手；以前一律記成最後接觸的右手，標註就被判成做不到、擱著 C 的標記。
    let (source, chart) = held_left_then_trio();
    let key = |area: &str| -> String {
        chart
            .notes
            .iter()
            .find(|n| match area {
                "C" => n.touch_area.as_deref().is_some_and(|a| a.starts_with('C')),
                _ => {
                    n.kind == "touch"
                        && n.touch_area.as_deref().is_some_and(|a| a.starts_with('B'))
                        && n.button.to_string() == area[1..]
                }
            })
            .unwrap_or_else(|| panic!("找不到 {area}"))
            .key
            .clone()
    };
    let hold = chart.notes.iter().find(|n| n.kind == "hold").unwrap();
    // 手掌半徑 0.35：右手一掌蓋得到 B2、B3，蓋不到 C（0.5 時連 C 一起蓋到，C 就不是連帶判定）
    let mut solver_config = SolverConfig::v3();
    solver_config.palm_radius = 0.35;
    let r = evaluate_annotation(EvaluateRequest {
        request_id: "group".into(),
        source: source.clone(),
        first_seconds: 0.0,
        solver_config,
        annotation: serde_json::from_value(serde_json::json!({
            "format": "maimotion-hand-annotation",
            "version": 1,
            "title": "test",
            "annotators": ["tester"],
            "chart": { "sha256": "", "firstSeconds": 0, "noteCount": 0, "source": source },
            "notes": [
                {"key": hold.key, "hand": "L"},
                {"key": key("B2"), "hand": "R"},
                {"key": key("B3"), "hand": "R"},
                {"key": key("C"), "hand": "L"},
            ],
        }))
        .unwrap(),
        model_solution: None,
        routes: vec![],
    });
    assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
    let center = chart
        .notes
        .iter()
        .find(|n| n.touch_area.as_deref().is_some_and(|a| a.starts_with('C')))
        .unwrap();
    let human = r.human_solution.as_ref().unwrap();
    let c = human
        .assignments
        .iter()
        .find(|a| a.note_id == center.id)
        .unwrap();
    assert_eq!((c.hand, c.part.as_str()), (Hand::L, "group"));
}
