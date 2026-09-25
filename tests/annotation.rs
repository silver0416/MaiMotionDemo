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
    evaluate_annotation(EvaluateRequest {
        request_id: "eval".into(),
        source: source.into(),
        first_seconds: 0.0,
        solver_config: SolverConfig::v3(),
        annotation: annotation(source, notes),
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
    let source = "(120){4}1/8,E";
    let r = evaluate(
        source,
        json!([
            {"key": key(source, 0), "hand": "L"},
            {"key": key(source, 1), "hand": "L"},
        ]),
    );
    assert_eq!(r.status, "human_infeasible");
    let d = r
        .diagnostics
        .iter()
        .find(|d| d.code == "annotation_infeasible")
        .unwrap();
    assert!(d.time_seconds.is_some());
    assert!(!d.note_ids.is_empty());
    assert!(r.model.is_some() && r.human.is_none());
}

/// 四月の雨 開頭：右手按住的 Hold 3 結束的同一瞬間，同一隻手從 2 開始滑 2q7。
/// 真人會提早放開 Hold 移過去，照標註求解不能因此判定做不到。
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
    let r = evaluate(
        source,
        json!([
            {"key": key(source, find("tap", 7)), "hand": "L"},
            {"key": key(source, slide), "hand": "R", "track": "R"},
            {"key": key(source, find("hold", 6)), "hand": "L"},
            {"key": key(source, find("hold", 5)), "hand": "L"},
            {"key": key(source, find("hold", 4)), "hand": "R"},
            {"key": key(source, hold3), "hand": "R"},
        ]),
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
