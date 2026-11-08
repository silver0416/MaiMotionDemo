//! 真人手順標註：穩定鍵、標註檔讀取與「照標註求解」的比對。
use mai_motion_core::*;
use serde_json::json;

fn chart(source: &str) -> Chart {
    parse_chart(source, 0.0).unwrap().chart
}

fn annotation(source: &str, notes: serde_json::Value) -> HandAnnotation {
    serde_json::from_value(json!({
        "format": "maimotion-hand-annotation",
        "version": 1,
        "title": "test",
        "annotators": ["tester"],
        "chart": { "sha256": "", "firstSeconds": 0, "noteCount": 0, "source": source },
        "notes": notes,
    }))
    .unwrap()
}

fn evaluate(source: &str, notes: serde_json::Value) -> EvaluateResponse {
    evaluate_with(source, notes, SolverConfig::v3())
}

fn evaluate_with(source: &str, notes: serde_json::Value, c: SolverConfig) -> EvaluateResponse {
    evaluate_annotation(EvaluateRequest {
        request_id: "eval".into(),
        source: source.into(),
        first_seconds: 0.0,
        solver_config: c,
        annotation: annotation(source, notes),
        model_solution: None,
        routes: vec![],
    })
}

fn key(source: &str, index: usize) -> String {
    chart(source).notes[index].key.clone()
}

#[test]
fn keys_are_stable_readable_and_unique() {
    let c = chart("(120){4}1,1/2,1-5[4:1],B3/C,3h[4:1],1pp5[4:1]*-5[4:1],E");
    let keys: Vec<&str> = c.notes.iter().map(|n| n.key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "0.000|tap|1",
            "0.500|tap|1",
            "0.500|tap|2",
            "1.000|slide|1-5",
            "1.500|touch|B3",
            "1.500|touch|C",
            "2.000|hold|3",
            "2.500|slide|1pp5",
            "3.000|slide|1-5",
        ]
    );
    let twice = chart("(120){4}1/1,E");
    assert_eq!(twice.notes[0].key, "0.000|tap|1");
    assert_eq!(twice.notes[1].key, "0.000|tap|1#2");
}

#[test]
fn human_hands_are_enforced_and_never_beat_the_model() {
    let source = "(120){8}1,8,2,7,3,6,E";
    // 全部用右手：慢速下做得到，但不會比模型自己的解更好。
    let notes: Vec<_> = (0..6)
        .map(|i| json!({"key": key(source, i), "hand": "R"}))
        .collect();
    let r = evaluate(source, json!(notes));
    assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
    assert_eq!((r.labeled, r.matched), (6, 6));
    let human = r.human.as_ref().unwrap();
    let model = r.model.as_ref().unwrap();
    assert!(human.cost >= model.cost - 1e-9);
    let solution = r.human_solution.as_ref().unwrap();
    assert!(solution.assignments.iter().all(|a| a.hand == Hand::R));
    assert_eq!(r.compared, 6);
    assert_eq!(r.agreed + r.divergences.len(), 6);
    // 和模型相同的手順，成本相同、全部吻合。
    let same: Vec<_> = (0..6)
        .map(|i| {
            let hand = solve_hand(source, i);
            json!({"key": key(source, i), "hand": hand})
        })
        .collect();
    let r = evaluate(source, json!(same));
    assert_eq!(r.agreed, 6);
    assert!((r.human.unwrap().cost - r.model.unwrap().cost).abs() < 1e-9);
}

fn solve_hand(source: &str, index: usize) -> &'static str {
    let r = analyze_chart(AnalyzeRequest {
        request_id: "m".into(),
        source: source.into(),
        first_seconds: 0.0,
        solver_config: SolverConfig::v3(),
    });
    let note = &r.chart.as_ref().unwrap().notes[index];
    match r.solutions[0]
        .assignments
        .iter()
        .find(|a| a.note_id == note.id)
        .unwrap()
        .hand
    {
        Hand::L => "L",
        Hand::R => "R",
    }
}

#[test]
fn impossible_annotation_reports_where_it_breaks() {
    // 隔一鍵的兩鍵一隻手搆不到（相鄰的見 adjacent_pair_labeled_with_one_hand_is_pressed_together）。
    let source = "(120){4}1/7,E";
    let r = evaluate(
        source,
        json!([
            {"key": key(source, 0), "hand": "L"},
            {"key": key(source, 1), "hand": "L"},
        ]),
    );
    assert_eq!(r.status, "human_infeasible");
    // 走不下去的那一刻擱著其中一顆的標記、照模型的手接下去，所以仍有照標註的解。
    let skipped: Vec<_> = r
        .diagnostics
        .iter()
        .filter(|d| d.code == "annotation_skipped")
        .collect();
    assert_eq!(skipped.len(), 1, "{:?}", r.diagnostics);
    assert_eq!(skipped[0].severity, "warning");
    assert_eq!(skipped[0].time_seconds, Some(0.0));
    assert_eq!(skipped[0].note_ids.len(), 1);
    assert!(r.model.is_some() && r.human.is_some());
    let c = chart(source);
    let kept = c
        .notes
        .iter()
        .find(|n| n.id != skipped[0].note_ids[0])
        .unwrap();
    let s = r.human_solution.as_ref().unwrap();
    assert!(s
        .assignments
        .iter()
        .any(|a| a.note_id == kept.id && a.hand == Hand::L));
}

/// 標了換手時間、右手卻要去按住長押：右手放開 Slide 暫停時還照標註，到最後才接不回來。
/// 卡住的那一刻沒有展開被標註擋掉，要退回最近一處違反標註的岔路、擱著那裡的標記才接得下去。
/// 模型本身也做不到的地方（兩手都按住 Hold 時又來一顆）照舊回報錯誤，不列出擱置。
#[test]
fn set_aside_reaches_back_when_the_conflict_shows_up_later() {
    let source = "(120){4}3-7[1:1],,,2h[1:1],E";
    let c = chart(source);
    let slide = c.notes.iter().find(|n| n.kind == "slide").unwrap();
    let hold = c.notes.iter().find(|n| n.kind == "hold").unwrap();
    let r = evaluate(
        source,
        json!([
            {"key": slide.key, "hand": "R", "track": "L", "handovers": [{"at": 1.0, "to": "R"}]},
            {"key": hold.key, "hand": "R"},
        ]),
    );
    assert_eq!(r.status, "human_infeasible");
    let skipped: Vec<_> = r
        .diagnostics
        .iter()
        .filter(|d| d.code == "annotation_skipped")
        .collect();
    assert_eq!(skipped.len(), 1, "{:?}", r.diagnostics);
    assert_eq!(skipped[0].note_ids, std::slice::from_ref(&slide.id));
    let s = r.human_solution.as_ref().unwrap();
    assert!(s
        .assignments
        .iter()
        .any(|a| a.note_id == hold.id && a.hand == Hand::R));
}

/// 模型本身走不下去（兩手都按著 Hold 時來了 Tap）不是標註的問題：比對用部分解，模型略過的
/// 那顆照標註時也略過、不比對（列成 model_skipped），其餘照常比，不擱置任何標記。
#[test]
fn notes_the_model_cannot_play_are_left_out_on_both_sides() {
    let source = "(120){4}1h[1:1]/7h[1:1],,5,,E";
    let tap = chart(source)
        .notes
        .into_iter()
        .find(|n| n.key == "1.000|tap|5")
        .unwrap();
    let r = evaluate(
        source,
        json!([
            {"key": "0.000|hold|1", "hand": "R"},
            {"key": "0.000|hold|7", "hand": "L"},
            {"key": "1.000|tap|5", "hand": "L"},
        ]),
    );
    assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
    assert!(r.diagnostics.iter().all(|d| d.code != "annotation_skipped"));
    let skipped: Vec<_> = r
        .diagnostics
        .iter()
        .filter(|d| d.code == "model_skipped")
        .collect();
    assert_eq!(skipped.len(), 1, "{:?}", r.diagnostics);
    assert_eq!(skipped[0].severity, "warning");
    assert_eq!(skipped[0].time_seconds, Some(1.0));
    assert_eq!(skipped[0].note_ids, std::slice::from_ref(&tap.id));
    // 兩個 Hold 照常比；Tap 沒有模型的手，不算進比對。
    assert_eq!(r.compared, 2);
    let s = r.human_solution.as_ref().unwrap();
    assert!(s.assignments.iter().all(|a| a.note_id != tap.id));
    assert_eq!(s.skipped.len(), 1);
    assert_eq!(s.skipped[0].note_id, tap.id);
}

/// 擱著標記之後照常往後解：後面再走不下去的地方另外列出，其餘的標記照舊遵守。
#[test]
fn every_place_the_annotation_breaks_is_listed_and_the_rest_is_followed() {
    let source = "(120){4}1/7,3,1/7,5,E";
    let r = evaluate(
        source,
        json!([
            {"key": "0.000|tap|1", "hand": "L"},
            {"key": "0.000|tap|7", "hand": "L"},
            {"key": "0.500|tap|3", "hand": "R"},
            {"key": "1.000|tap|1", "hand": "L"},
            {"key": "1.000|tap|7", "hand": "L"},
            {"key": "1.500|tap|5", "hand": "L"},
        ]),
    );
    assert_eq!(r.status, "human_infeasible");
    let skipped: Vec<_> = r
        .diagnostics
        .iter()
        .filter(|d| d.code == "annotation_skipped")
        .collect();
    let times: Vec<_> = skipped.iter().map(|d| d.time_seconds).collect();
    assert_eq!(times, [Some(0.0), Some(1.0)], "{:?}", r.diagnostics);
    assert!(skipped.iter().all(|d| d.note_ids.len() == 1));
    let c = chart(source);
    let s = r.human_solution.as_ref().unwrap();
    let hand = |key: &str| {
        let id = &c.notes.iter().find(|n| n.key == key).unwrap().id;
        s.assignments
            .iter()
            .find(|a| &a.note_id == id)
            .unwrap()
            .hand
    };
    assert_eq!(hand("0.500|tap|3"), Hand::R);
    assert_eq!(hand("1.500|tap|5"), Hand::L);
    for (d, pair) in skipped.iter().zip([
        ["0.000|tap|1", "0.000|tap|7"],
        ["1.000|tap|1", "1.000|tap|7"],
    ]) {
        let ids: Vec<_> = pair
            .iter()
            .map(|key| &c.notes.iter().find(|n| &n.key == key).unwrap().id)
            .collect();
        assert!(ids.contains(&&d.note_ids[0]));
        let kept = pair[ids.iter().position(|id| **id != d.note_ids[0]).unwrap()];
        assert_eq!(hand(kept), Hand::L);
    }
    assert!(s
        .warnings
        .iter()
        .any(|w| w.contains("照標註有 2 處走不下去")));
}

/// 四月の雨 開頭：右手按住的 Hold 3 結束的同一瞬間，同一隻手從 2 開始滑 2q7。
/// 真人會提早放開 Hold 移過去；打開 holdEarlyRelease 時照標註求解不能因此判定做不到。
#[test]
fn hold_ending_as_the_same_hands_slide_starts_is_released_early() {
    let source = "(79){16}7,,,,2q7[8:1]/6h[16:1],5h[16:1],4h[16:1],3h[16:1],,,,,,,,,E";
    let c = chart(source);
    let find = |kind: &str, button: u8| {
        c.notes
            .iter()
            .position(|n| n.kind == kind && n.button == button)
            .unwrap()
    };
    let (slide, hold3) = (find("slide", 2), find("hold", 3));
    assert!((c.notes[hold3].end_seconds - c.notes[slide].motion_start.unwrap()).abs() < 1e-9);
    let r = evaluate_with(
        source,
        json!([
            {"key": key(source, find("tap", 7)), "hand": "L"},
            {"key": key(source, slide), "hand": "R", "track": "R"},
            {"key": key(source, find("hold", 6)), "hand": "L"},
            {"key": key(source, find("hold", 5)), "hand": "L"},
            {"key": key(source, find("hold", 4)), "hand": "R"},
            {"key": key(source, hold3), "hand": "R"},
        ]),
        SolverConfig {
            hold_early_release: true,
            ..SolverConfig::v3()
        },
    );
    assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
    assert!(r.human.is_some());
}

#[test]
fn slide_handover_and_track_hand_are_followed() {
    // 長 Slide 沒有其他音符需要手：V3 平常不會換手，但標註明確要求時照做。
    let source = "(120){4}1-5[1:1],,,,,E";
    let note = chart(source).notes[0].clone();
    let middle = (note.motion_start.unwrap() + note.motion_end.unwrap()) / 2.0;
    let r = evaluate(
        source,
        json!([{"key": note.key, "hand": "L", "track": "L", "handovers": [{"at": middle, "to": "R"}]}]),
    );
    assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
    let s = r.human_solution.unwrap();
    assert!(s
        .handovers
        .iter()
        .any(|h| h.note_id == note.id && h.to == Hand::R));
    let slides: Vec<_> = s.assignments.iter().filter(|a| a.part == "slide").collect();
    let first = slides
        .iter()
        .min_by(|a, b| a.start_seconds.total_cmp(&b.start_seconds))
        .unwrap();
    assert_eq!(first.hand, Hand::L);
    for a in &slides {
        if a.end_seconds < middle - 0.1 {
            assert_eq!(a.hand, Hand::L);
        }
        if a.start_seconds > middle + 0.1 {
            assert_eq!(a.hand, Hand::R);
        }
    }
    // 只標滑行手、不換手：整條都要是右手。
    let r = evaluate(source, json!([{"key": note.key, "track": "R"}]));
    assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
    let s = r.human_solution.unwrap();
    assert!(s
        .assignments
        .iter()
        .filter(|a| a.part == "slide")
        .all(|a| a.hand == Hand::R));
}

#[test]
fn wifi_can_be_pinned_to_one_hand_or_both() {
    let source = "(120){4}1w5[4:2],E";
    let k = key(source, 0);
    let r = evaluate(source, json!([{"key": k, "track": "L"}]));
    assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
    let s = r.human_solution.unwrap();
    assert!(s
        .assignments
        .iter()
        .filter(|a| a.part == "slide")
        .all(|a| a.hand == Hand::L));
    let r = evaluate(source, json!([{"key": k, "track": "LR"}]));
    assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
}

#[test]
fn prefilled_unsure_and_unknown_keys_do_not_constrain() {
    let source = "(120){4}1/8,E";
    let r = evaluate(
        source,
        json!([
            {"key": key(source, 0), "hand": "L", "prefilled": true},
            {"key": key(source, 1), "hand": "L", "confidence": "unsure"},
            {"key": "9.999|tap|4", "hand": "R"},
        ]),
    );
    assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
    assert_eq!(r.labeled, 2);
    assert_eq!(r.matched, 1);
    assert_eq!(r.unmatched_keys, ["9.999|tap|4"]);
    assert_eq!(r.compared, 0);
}

/// 有起點的 Slide：起點與滑行各有自己的信心。只有「確定」的那一步限制求解；比對時「皆可」不論模型用哪隻手都算吻合。
#[test]
fn slide_start_and_track_have_their_own_confidence() {
    let source = "(120){4}1-5[4:1],,,,E";
    let k = key(source, 0);
    let slide_hands = |r: &EvaluateResponse| -> Vec<Hand> {
        r.human_solution
            .as_ref()
            .unwrap()
            .assignments
            .iter()
            .filter(|a| a.part == "slide")
            .map(|a| a.hand)
            .collect()
    };
    // 起點確定、滑行皆可：兩步都比對，滑行一定算吻合。
    let r = evaluate(
        source,
        json!([{"key": k, "hand": "L", "track": "R", "trackConfidence": "either"}]),
    );
    assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
    assert_eq!(r.compared, 2);
    assert!(r.divergences.iter().all(|d| d.part == "hand"));
    // 起點皆可、滑行確定：起點一定算吻合，只有滑行可能不同，而且照標註用右手滑。
    let r = evaluate(
        source,
        json!([{"key": k, "hand": "L", "track": "R", "confidence": "either", "trackConfidence": "sure"}]),
    );
    assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
    assert_eq!(r.compared, 2);
    assert!(r.divergences.iter().all(|d| d.part == "track"));
    assert!(slide_hands(&r).iter().all(|&h| h == Hand::R));
    // 舊標註檔沒有 trackConfidence：整顆共用 confidence，兩步都皆可時全部吻合。
    let r = evaluate(
        source,
        json!([{"key": k, "hand": "L", "track": "R", "confidence": "either"}]),
    );
    assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
    assert_eq!((r.compared, r.agreed), (2, 2));
    assert!(r.divergences.is_empty());
}

/// 星星頭真人標了、滑行還是預填（prefilledPart = track）：起點照樣限制求解與比對，滑行不算答案；
/// 反過來也一樣。舊標註檔只有 prefilled，整顆都是預填、不列入。
#[test]
fn prefilled_step_is_dropped_but_the_human_step_of_the_same_slide_counts() {
    let source = "(120){4}1-5[4:1],,,,E";
    let k = key(source, 0);
    let hands = |r: &EvaluateResponse, part: &str| -> Vec<Hand> {
        r.human_solution
            .as_ref()
            .unwrap()
            .assignments
            .iter()
            .filter(|a| a.part == part)
            .map(|a| a.hand)
            .collect()
    };
    let head_human =
        json!([{"key": k, "hand": "L", "track": "R", "prefilled": true, "prefilledPart": "track"}]);
    let stored = serde_json::to_value(&annotation(source, head_human.clone()).notes[0]).unwrap();
    assert_eq!(stored["prefilledPart"], "track");
    let r = evaluate(source, head_human);
    assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
    assert_eq!((r.labeled, r.matched, r.compared), (1, 1, 1));
    assert!(r.divergences.iter().all(|d| d.part == "hand"));
    assert_eq!(hands(&r, "head"), [Hand::L]);

    let r = evaluate(
        source,
        json!([{"key": k, "hand": "L", "track": "R", "prefilled": true, "prefilledPart": "hand"}]),
    );
    assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
    assert_eq!((r.labeled, r.compared), (1, 1));
    assert!(r.divergences.iter().all(|d| d.part == "track"));
    assert!(hands(&r, "slide").iter().all(|&h| h == Hand::R));

    let r = evaluate(
        source,
        json!([{"key": k, "hand": "L", "track": "R", "prefilled": true}]),
    );
    assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
    assert_eq!((r.labeled, r.compared), (0, 0));
}

#[test]
fn wrong_format_is_rejected_with_a_message() {
    let source = "(120){4}1,E";
    let mut a = annotation(source, json!([]));
    a.format = "something-else".into();
    let r = evaluate_annotation(EvaluateRequest {
        request_id: "bad".into(),
        source: source.into(),
        first_seconds: 0.0,
        solver_config: SolverConfig::v3(),
        annotation: a,
        model_solution: None,
        routes: vec![],
    });
    assert_eq!(r.status, "invalid");
    assert!(r.diagnostics[0]
        .message
        .contains("maimotion-hand-annotation"));
}

/// 某條 Slide 的滑行段中，前後兩段之間的空檔（手放開後再接回）。
fn slide_gaps(solution: &Solution, note_id: &str) -> Vec<(Hand, f64, Hand, f64)> {
    let mut parts: Vec<_> = solution
        .assignments
        .iter()
        .filter(|a| a.note_id == note_id && a.part == "slide")
        .collect();
    parts.sort_by(|a, b| a.start_seconds.total_cmp(&b.start_seconds));
    parts
        .windows(2)
        .filter(|p| p[1].start_seconds - p[0].end_seconds > 0.02)
        .map(|p| (p[0].hand, p[0].end_seconds, p[1].hand, p[1].start_seconds))
        .collect()
}

/// 四月の雨 24.3 秒：右手滑長 Slide 8q1 的前段後放開，兩手去打 8～2 的連打，
/// 最後左手接回去補完。判定進度會保留，放開再接回是合法打法。
#[test]
fn long_slide_can_be_paused_and_finished_by_the_other_hand() {
    let source = "(79){16}8q1[4:6],8,8,8,7,7,7,7,6,6,6,6,5,5,5,5,\n4,4,4,4,3,3,3,3,2,2,2,2,,,,,E";
    let c = chart(source);
    let slide = c.notes.iter().position(|n| n.kind == "slide").unwrap();
    let mut marks = vec![json!({
        "key": c.notes[slide].key,
        "hand": "L",
        "track": "R",
        "handovers": [{"at": 3.7, "to": "L"}],
    })];
    for note in c.notes.iter().filter(|n| n.kind == "tap") {
        let hand = if note.button <= 4 { "R" } else { "L" };
        marks.push(json!({"key": note.key, "hand": hand}));
    }
    let r = evaluate(source, json!(marks));
    assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
    let gaps = slide_gaps(r.human_solution.as_ref().unwrap(), &c.notes[slide].id);
    assert_eq!(gaps.len(), 1, "{gaps:?}");
    let (before, paused_at, after, resumed_at) = gaps[0];
    assert_eq!((before, after), (Hand::R, Hand::L));
    // 右手在連打 4 之前放開；左手在自己的連打（5）結束後才接回。
    let first_four = c
        .notes
        .iter()
        .find(|n| n.kind == "tap" && n.button == 4)
        .unwrap();
    let last_five = c
        .notes
        .iter()
        .filter(|n| n.kind == "tap" && n.button == 5)
        .next_back()
        .unwrap();
    assert!(paused_at <= first_four.time_seconds, "{gaps:?}");
    assert!(resumed_at >= last_five.time_seconds, "{gaps:?}");
}

/// 短 Slide 放開再接回只是規避追蹤成本的捷徑：模型不應該這樣打。
#[test]
fn short_slides_are_not_paused() {
    let source = "(79){16}1p6[4:1],,8,,6>1[4:1],7,7,,1p6[4:1],,8,,6,7b,,7b,\n8q3[4:1],,1,,3<8[4:1],,2,,8q3[4:1],,1,,3,,,,E";
    let r = analyze_chart(AnalyzeRequest {
        request_id: "short".into(),
        source: source.into(),
        first_seconds: 0.0,
        solver_config: SolverConfig::v3(),
    });
    let c = r.chart.as_ref().unwrap();
    for note in c.notes.iter().filter(|n| n.kind == "slide") {
        let gaps = slide_gaps(&r.solutions[0], &note.id);
        assert!(gaps.is_empty(), "{} {gaps:?}", note.key);
    }
}

/// 前端送來現有分析的第一名（去掉動作軌跡、經過 JSON）時，比對結果與在核心重算一樣。
#[test]
fn supplied_model_solution_gives_the_same_evaluation() {
    let source = "(150){8}1,2,3/6,4-8[8:1],5,7,B3/C,2h[4:1],1,8,E";
    let notes = json!([
        {"key": key(source, 0), "hand": "L"},
        {"key": key(source, 1), "hand": "R"},
        {"key": key(source, 5), "hand": "L", "track": "R"},
    ]);
    let analyzed = analyze_chart(AnalyzeRequest {
        request_id: "a".into(),
        source: source.into(),
        first_seconds: 0.0,
        solver_config: SolverConfig::v3(),
    });
    let mut top = serde_json::to_value(&analyzed.solutions[0]).unwrap();
    top["leftSegments"] = json!([]);
    top["rightSegments"] = json!([]);
    top["palmPlacements"] = json!([]);
    let supplied = evaluate_annotation(EvaluateRequest {
        request_id: "eval".into(),
        source: source.into(),
        first_seconds: 0.0,
        solver_config: SolverConfig::v3(),
        annotation: annotation(source, notes.clone()),
        model_solution: Some(serde_json::from_value(top).unwrap()),
        routes: vec![],
    });
    let solved = evaluate(source, notes);
    let summary = |r: &EvaluateResponse| {
        json!({
            "status": r.status,
            "compared": r.compared,
            "agreed": r.agreed,
            "divergences": r.divergences,
            "model": r.model,
            "human": r.human,
        })
    };
    assert_eq!(solved.status, "ok", "{:?}", solved.diagnostics);
    assert!(solved.compared > 0);
    assert_eq!(summary(&supplied), summary(&solved));
}

/// 打法路線（2026-11-08）：主線和各條分支各自和模型比，取吻合率最高的那條；
/// 吻合數、不同處與照標註求解都照那條。同分時取前面的（主線最前）。
#[test]
fn best_matching_route_is_used() {
    let source = "(120){8}1,8,2,7,3,6,E";
    let c = chart(source);
    let analyzed = analyze_chart(AnalyzeRequest {
        request_id: "a".into(),
        source: source.into(),
        first_seconds: 0.0,
        solver_config: SolverConfig::v3(),
    });
    let model = &analyzed.solutions[0];
    let hand_of = |id: &str| {
        model
            .assignments
            .iter()
            .find(|a| a.note_id == id)
            .unwrap()
            .hand
    };
    let name = |h: Hand| if h == Hand::L { "L" } else { "R" };
    let other = |h: Hand| if h == Hand::L { "R" } else { "L" };
    // 主線前兩顆和模型相反；分支 b 全部和模型相同；分支 c 和主線一樣。
    let main: Vec<_> = c
        .notes
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let h = hand_of(&n.id);
            json!({"key": n.key, "hand": if i < 2 { other(h) } else { name(h) }})
        })
        .collect();
    let agree: Vec<_> = c
        .notes
        .iter()
        .map(|n| json!({"key": n.key, "hand": name(hand_of(&n.id))}))
        .collect();
    let route = |id: &str, notes: &Vec<serde_json::Value>| -> RouteAnnotation {
        serde_json::from_value(json!({"id": id, "name": id, "notes": notes})).unwrap()
    };
    let request = |routes: Vec<RouteAnnotation>| EvaluateRequest {
        request_id: "routes".into(),
        source: source.into(),
        first_seconds: 0.0,
        solver_config: SolverConfig::v3(),
        annotation: annotation(source, json!(main)),
        model_solution: None,
        routes,
    };

    let r = evaluate_annotation(request(vec![route("c", &main), route("b", &agree)]));
    assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
    assert_eq!(r.route, "b");
    let scores: Vec<_> = r
        .route_scores
        .iter()
        .map(|s| (s.id.as_str(), s.agreed, s.compared))
        .collect();
    assert_eq!(scores, [(MAIN_ROUTE, 4, 6), ("c", 4, 6), ("b", 6, 6)]);
    assert_eq!((r.agreed, r.compared, r.divergences.len()), (6, 6, 0));
    assert_eq!((r.labeled, r.matched), (6, 6));
    let human = r.human_solution.as_ref().unwrap();
    for n in &c.notes {
        let hand = human
            .assignments
            .iter()
            .find(|a| a.note_id == n.id)
            .unwrap()
            .hand;
        assert_eq!(hand, hand_of(&n.id), "{}", n.key);
    }

    // 只有和主線一樣的分支：同分取主線。
    let tie = evaluate_annotation(request(vec![route("c", &main)]));
    assert_eq!(tie.route, MAIN_ROUTE);
    assert_eq!((tie.agreed, tie.compared, tie.divergences.len()), (4, 6, 2));
    // 沒有分支：照舊只比主線，不列各條路線的分數。
    let single = evaluate(source, json!(main));
    assert_eq!(single.route, MAIN_ROUTE);
    assert!(single.chosen_routes.is_empty());
    assert!(single.route_scores.is_empty());
    assert_eq!((single.agreed, single.compared), (4, 6));
}

/// 每一段各挑（2026-11-09）：不同段的走法各自挑，合起來是整首吻合率最高的組合；同率取比對步數多的。
/// 沒帶段的走法照舊整首只挑一條。
#[test]
fn each_section_picks_its_best_route() {
    let source = "(120){8}1,8,2,7,3,6,E";
    let c = chart(source);
    let analyzed = analyze_chart(AnalyzeRequest {
        request_id: "a".into(),
        source: source.into(),
        first_seconds: 0.0,
        solver_config: SolverConfig::v3(),
    });
    let model = &analyzed.solutions[0];
    let hand_of = |id: &str| {
        model
            .assignments
            .iter()
            .find(|a| a.note_id == id)
            .unwrap()
            .hand
    };
    let name = |h: Hand| if h == Hand::L { "L" } else { "R" };
    let other = |h: Hand| if h == Hand::L { "R" } else { "L" };
    // 主線第 1、2 顆與第 5、6 顆和模型相反（2／6）。第一段的 b 改對第 1、2 顆；第二段的 c 改對第 5、6 顆、
    // d 只改對第 5 顆、e 把第 5、6 顆標成不確定（不比對，b＋e 也是 100%，但比對步數比 b＋c 少）。
    let marks = |right: &[usize], unsure: &[usize]| -> Vec<serde_json::Value> {
        c.notes
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let h = hand_of(&n.id);
                let wrong = matches!(i, 0 | 1 | 4 | 5) && !right.contains(&i);
                let mut mark =
                    json!({"key": n.key, "hand": if wrong { other(h) } else { name(h) }});
                if unsure.contains(&i) {
                    mark["confidence"] = json!("unsure");
                }
                mark
            })
            .collect()
    };
    let route =
        |id: &str, group: Option<usize>, notes: Vec<serde_json::Value>| -> RouteAnnotation {
            serde_json::from_value(json!({"id": id, "name": id, "notes": notes, "group": group}))
                .unwrap()
        };
    let request = |groups: [Option<usize>; 4]| EvaluateRequest {
        request_id: "sections".into(),
        source: source.into(),
        first_seconds: 0.0,
        solver_config: SolverConfig::v3(),
        annotation: annotation(source, json!(marks(&[], &[]))),
        model_solution: None,
        routes: vec![
            route("b", groups[0], marks(&[0, 1], &[])),
            route("e", groups[1], marks(&[], &[4, 5])),
            route("d", groups[2], marks(&[4], &[])),
            route("c", groups[3], marks(&[4, 5], &[])),
        ],
    };

    let r = evaluate_annotation(request([Some(0), Some(1), Some(1), Some(1)]));
    assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
    assert_eq!(r.chosen_routes, ["b", "c"]);
    assert_eq!(r.route, "b+c");
    let scores: Vec<_> = r
        .route_scores
        .iter()
        .map(|s| (s.id.as_str(), s.agreed, s.compared))
        .collect();
    assert_eq!(
        scores,
        [
            (MAIN_ROUTE, 2, 6),
            ("b", 4, 6),
            ("e", 2, 4),
            ("d", 3, 6),
            ("c", 4, 6)
        ]
    );
    assert_eq!((r.agreed, r.compared, r.divergences.len()), (6, 6, 0));
    assert_eq!((r.labeled, r.matched), (6, 6));
    let human = r.human_solution.as_ref().unwrap();
    for n in &c.notes {
        let hand = human
            .assignments
            .iter()
            .find(|a| a.note_id == n.id)
            .unwrap()
            .hand;
        assert_eq!(hand, hand_of(&n.id), "{}", n.key);
    }

    // 沒帶段：整首只挑一條（2026-11-08 的算法）；b 和 c 都是 4／6，取前面的 b。
    let single = evaluate_annotation(request([None; 4]));
    assert_eq!(single.chosen_routes, ["b"]);
    assert_eq!(single.route, "b");
    assert_eq!((single.agreed, single.compared), (4, 6));
}

/// 一顆 Slide 的起點與滑行分屬不同段時，組合裡起點照起點那段挑的走法、滑行照滑行那段挑的走法。
#[test]
fn slide_parts_follow_their_own_sections() {
    let source = "(120){4}1-5[4:1],,,,3,E";
    let c = chart(source);
    let analyzed = analyze_chart(AnalyzeRequest {
        request_id: "a".into(),
        source: source.into(),
        first_seconds: 0.0,
        solver_config: SolverConfig::v3(),
    });
    let model = &analyzed.solutions[0];
    let slide = c.notes.iter().find(|n| n.kind == "slide").unwrap();
    let tap = c.notes.iter().find(|n| n.kind != "slide").unwrap();
    let first = |id: &str, slide_part: bool| {
        model
            .assignments
            .iter()
            .filter(|a| a.note_id == id && (a.part == "slide") == slide_part)
            .min_by(|a, b| a.start_seconds.total_cmp(&b.start_seconds))
            .unwrap()
            .hand
    };
    let name = |h: Hand| if h == Hand::L { "L" } else { "R" };
    let other = |h: Hand| if h == Hand::L { "R" } else { "L" };
    let (contact, track) = (first(&slide.id, false), first(&slide.id, true));
    let marks = |contact_right: bool, track_right: bool| {
        json!([
            {"key": slide.key, "hand": if contact_right { name(contact) } else { other(contact) },
             "track": if track_right { name(track) } else { other(track) }},
            {"key": tap.key, "hand": name(first(&tap.id, false))},
        ])
    };
    let route = |id: &str, group: usize, notes: serde_json::Value| -> RouteAnnotation {
        serde_json::from_value(json!({"id": id, "name": id, "notes": notes, "group": group}))
            .unwrap()
    };
    let r = evaluate_annotation(EvaluateRequest {
        request_id: "slide".into(),
        source: source.into(),
        first_seconds: 0.0,
        solver_config: SolverConfig::v3(),
        annotation: annotation(source, marks(false, false)),
        model_solution: None,
        routes: vec![
            route("start", 0, marks(true, false)),
            route("slide", 1, marks(false, true)),
        ],
    });
    assert_eq!(r.chosen_routes, ["start", "slide"]);
    let scores: Vec<_> = r
        .route_scores
        .iter()
        .map(|s| (s.agreed, s.compared))
        .collect();
    assert_eq!(scores, [(1, 3), (2, 3), (2, 3)]);
    assert_eq!((r.agreed, r.compared, r.divergences.len()), (3, 3, 0));
}

/// Comet Panto Men! 111.529：同時的相鄰兩鍵都標同一隻手，那隻手張開、落在兩鍵中間一起按。
/// 模型自己會兩手各按一顆，不這樣分。
#[test]
fn adjacent_pair_labeled_with_one_hand_is_pressed_together() {
    let source = "(120){4}1/8,E";
    let r = evaluate(
        source,
        json!([
            {"key": key(source, 0), "hand": "L"},
            {"key": key(source, 1), "hand": "L"},
        ]),
    );
    assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
    let s = r.human_solution.as_ref().unwrap();
    assert!(s.assignments.iter().all(|a| a.hand == Hand::L));
    assert!(s.warnings.iter().any(|w| w.contains("相鄰兩鍵")));
    let c = chart(source);
    let middle = c.notes[0].position.lerp(c.notes[1].position, 0.5);
    let press = s.left_segments.iter().find(|g| g.mode == "tap").unwrap();
    assert!(press
        .samples
        .iter()
        .all(|p| p.point().distance(middle) < 1e-9));
    assert_ne!(solve_hand(source, 0), solve_hand(source, 1));
}

/// Luminaria 114～117 秒：滑行只標了開始滑的右手、沒標換手時間，右手中途卻要去按住長押。
/// 照整條同一隻手走不下去，這條改成只限定開始滑的手，由左手補完。做得到的照舊整條同一隻手
/// （見 slide_handover_and_track_hand_are_followed 最後一段）。
#[test]
fn unmarked_handover_is_allowed_only_where_one_hand_cannot_finish() {
    let source = "(120){4}3-7[1:1],,,2h[1:1],E";
    let c = chart(source);
    let slide = c.notes.iter().find(|n| n.kind == "slide").unwrap();
    let hold = c.notes.iter().find(|n| n.kind == "hold").unwrap();
    assert!(
        hold.time_seconds < slide.motion_end.unwrap()
            && hold.end_seconds > slide.motion_end.unwrap()
    );
    let r = evaluate(
        source,
        json!([
            {"key": slide.key, "hand": "R", "track": "R"},
            {"key": hold.key, "hand": "R"},
        ]),
    );
    assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
    let s = r.human_solution.as_ref().unwrap();
    assert!(s.warnings.iter().any(|w| w.contains("只限定開始滑的手")));
    let mut parts: Vec<_> = s
        .assignments
        .iter()
        .filter(|a| a.note_id == slide.id && a.part == "slide")
        .collect();
    parts.sort_by(|a, b| a.start_seconds.total_cmp(&b.start_seconds));
    assert_eq!(parts[0].hand, Hand::R);
    assert!(parts.iter().any(|a| a.hand == Hand::L));
    // 標了換手時間的照時間表，不另外放寬：左手開始、1 秒換右手畫完，右手就按不到長押。
    let r = evaluate(
        source,
        json!([
            {"key": slide.key, "hand": "R", "track": "L", "handovers": [{"at": 1.0, "to": "R"}]},
            {"key": hold.key, "hand": "R"},
        ]),
    );
    assert_eq!(r.status, "human_infeasible");
}
