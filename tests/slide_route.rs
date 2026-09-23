//! V3 依 Slide 判定佇列（MajdataPlay）抄近：路線、完成時刻、一手覆蓋與順手點 Tap。
use mai_motion_core::*;

fn run(source: &str, solver_config: SolverConfig) -> AnalyzeResponse {
    analyze_chart(AnalyzeRequest {
        request_id: "route".into(),
        source: source.into(),
        first_seconds: 0.0,
        solver_config,
    })
}

fn names(queue: &[JudgeArea]) -> String {
    queue
        .iter()
        .map(|a| {
            format!(
                "{}{}",
                a.sensors.join("|"),
                if a.skippable { "" } else { "!" }
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn queue_of(source: &str) -> String {
    let r = run(source, SolverConfig::default());
    names(&r.chart.unwrap().paths[0].judge_areas)
}

fn continuity(segments: &[MotionSegment]) {
    for pair in segments.windows(2) {
        assert!((pair[0].end_seconds - pair[1].start_seconds).abs() < 1e-7);
        assert!(
            pair[0]
                .samples
                .last()
                .unwrap()
                .point()
                .distance(pair[1].samples[0].point())
                < 1e-7
        );
    }
}

fn traced(segments: &[MotionSegment], note: &str) -> Vec<Point> {
    let mut points: Vec<Point> = vec![];
    for segment in segments
        .iter()
        .filter(|s| s.mode == "slide" && s.note_id.as_deref() == Some(note))
    {
        for sample in &segment.samples {
            let p = sample.point();
            if points.last().is_none_or(|q| q.distance(p) > 1e-12) {
                points.push(p);
            }
        }
    }
    points
}

fn length(points: &[Point]) -> f64 {
    points.windows(2).map(|w| w[0].distance(w[1])).sum()
}

fn judge_time(chart: &Chart, note: &Note) -> f64 {
    let path = chart
        .paths
        .iter()
        .find(|p| Some(&p.id) == note.path_id.as_ref())
        .unwrap();
    let (start, end) = (note.motion_start.unwrap(), note.motion_end.unwrap());
    start + (end - start) * path.judge_progress
}

#[test]
fn judge_queues_follow_majdata_tables_with_mirror_and_rotation() {
    assert_eq!(queue_of("(120){4}1-5[4:1],E"), "A1 B1 C B5 A5");
    assert_eq!(queue_of("(120){4}3-5[4:1],E"), "A3 A4|B4! A5");
    assert_eq!(queue_of("(120){4}1>4[4:1],E"), "A1 A2 A3 A4");
    // 下半的 > 逆時針，用鏡射的圓弧表。
    assert_eq!(queue_of("(120){4}4>1[4:1],E"), "A4 A3 A2 A1");
    assert_eq!(queue_of("(120){4}1^3[4:1],E"), "A1 A2! A3");
    assert_eq!(
        queue_of("(120){4}1pp5[4:1],E"),
        "A1 B1 C B4 A3 A2 B1 C B5 A5"
    );
    assert_eq!(queue_of("(120){4}1q5[4:1],E"), "A1 B2 B3 B4 A5");
    assert_eq!(queue_of("(120){4}1z5[4:1],E"), "A1 B2 B3 C B7 B6 A5");
    assert_eq!(queue_of("(120){4}8V64[4:1],E"), "A8 B7|A7! A6 B5|A5! A4");
    assert_eq!(queue_of("(120){4}1-3-5[4:1],E"), "A1 A2|B2! A3 A4|B4! A5");
    let r = run("(120){4}1w5[4:2],E", SolverConfig::default());
    let path = &r.chart.unwrap().paths[0];
    assert_eq!(names(&path.judge_areas), "A1 B1 C A5|B5");
    assert_eq!(names(&path.branch_judge_areas[0]), "A1 B2 B3 A4|D5");
    assert_eq!(names(&path.branch_judge_areas[1]), "A1 B8 B7 A6|D6");
}

#[test]
fn v3_hand_cuts_across_skippable_areas_and_finishes_at_the_judgment_time() {
    for source in [
        "(120){4}1-5[4:1],E",
        "(120){4}1>5[4:1],E",
        "(120){4}1pp5[4:1],E",
        "(120){4}1V72[4:1],E",
        "(120){4}1s5[4:1],E",
        "(120){4}2q6[4:1],E",
        "(120){4}3<3[2:1],E",
        "(120){4}6^8[4:1],E",
    ] {
        let r = run(source, SolverConfig::v3());
        assert_eq!(r.status, "ok", "{source}: {:?}", r.diagnostics);
        let chart = r.chart.as_ref().unwrap();
        let note = &chart.notes[0];
        let path = &chart.paths[0];
        let nominal = length(
            &path
                .samples
                .iter()
                .map(|s| Point { x: s.x, y: s.y })
                .collect::<Vec<_>>(),
        );
        let judge = judge_time(chart, note);
        for s in &r.solutions {
            continuity(&s.left_segments);
            continuity(&s.right_segments);
            let owner = s
                .assignments
                .iter()
                .find(|a| a.part == "slide")
                .unwrap()
                .hand;
            let end = s
                .assignments
                .iter()
                .filter(|a| a.part == "slide")
                .map(|a| a.end_seconds)
                .fold(0.0, f64::max);
            assert!((end - judge).abs() < 1e-9, "{source}: {end} vs {judge}");
            let segments = if owner == Hand::L {
                &s.left_segments
            } else {
                &s.right_segments
            };
            let points = traced(segments, &note.id);
            assert!(
                simulate_route(&points, &[&path.judge_areas], None).is_some(),
                "{source}: 指尖軌跡必須依判定佇列完成"
            );
            assert!(
                length(&points) < nominal - 0.1,
                "{source}: 應比名目路徑短 {} / {nominal}",
                length(&points)
            );
        }
        // V1 仍沿名目路徑追到星星終點。
        let legacy = run(source, SolverConfig::default());
        let end = legacy.solutions[0]
            .assignments
            .iter()
            .filter(|a| a.part == "slide")
            .map(|a| a.end_seconds)
            .fold(0.0, f64::max);
        assert!((end - note.motion_end.unwrap()).abs() < 1e-9);
    }
}

#[test]
fn v3_chained_slides_share_one_continuous_shortcut() {
    let r = run("(120){4}1-3[4:1]-5[4:1],E", SolverConfig::v3());
    assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
    let chart = r.chart.as_ref().unwrap();
    let (first, second) = (&chart.notes[0], &chart.notes[1]);
    let judge = judge_time(chart, second);
    let queue: Vec<JudgeArea> = chart.paths[0]
        .judge_areas
        .iter()
        .chain(chart.paths[1].judge_areas.iter().skip(1))
        .cloned()
        .collect();
    for s in &r.solutions {
        continuity(&s.left_segments);
        continuity(&s.right_segments);
        let first_end = s
            .assignments
            .iter()
            .filter(|a| a.note_id == first.id && a.part == "slide")
            .map(|a| a.end_seconds)
            .fold(0.0, f64::max);
        assert!((first_end - first.motion_end.unwrap()).abs() < 1e-9);
        let end = s
            .assignments
            .iter()
            .filter(|a| a.note_id == second.id && a.part == "slide")
            .map(|a| a.end_seconds)
            .fold(0.0, f64::max);
        assert!((end - judge).abs() < 1e-9);
        // 接點可能換手；依時間把兩隻手在這條 Slide 上的軌跡接起來。
        let mut pieces: Vec<&MotionSegment> = s
            .left_segments
            .iter()
            .chain(&s.right_segments)
            .filter(|g| {
                g.mode == "slide"
                    && (g.note_id.as_deref() == Some(first.id.as_str())
                        || g.note_id.as_deref() == Some(second.id.as_str()))
            })
            .collect();
        pieces.sort_by(|a, b| a.start_seconds.total_cmp(&b.start_seconds));
        let mut points: Vec<Point> = vec![];
        for piece in pieces {
            for sample in &piece.samples {
                let p = sample.point();
                if let Some(q) = points.last() {
                    if q.distance(p) <= 1e-12 {
                        continue;
                    }
                }
                points.push(p);
            }
        }
        assert!(simulate_route(&points, &[&queue], None).is_some());
    }
}

#[test]
fn v3_one_spread_hand_covers_two_close_simultaneous_slides() {
    // 7 鍵被長 Hold 佔住，同一顆星星分出的兩條 Slide 只能由另一隻手張開同時覆蓋。
    let source = "(120){4}1-5[4:1]*-4[4:1]/7h[1:1],E";
    let r = run(source, SolverConfig::v3());
    assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
    let chart = r.chart.as_ref().unwrap();
    let paths = [&chart.paths[0], &chart.paths[1]];
    for s in &r.solutions {
        continuity(&s.left_segments);
        continuity(&s.right_segments);
        let hold = s
            .assignments
            .iter()
            .find(|a| a.note_id == "n2")
            .unwrap()
            .hand;
        let slides: Vec<_> = s.assignments.iter().filter(|a| a.part == "slide").collect();
        assert!(slides.iter().all(|a| a.hand != hold));
        let lead = slides[0].hand;
        let segments = if lead == Hand::L {
            &s.left_segments
        } else {
            &s.right_segments
        };
        let points: Vec<Point> = ["n0", "n1"]
            .iter()
            .map(|id| traced(segments, id))
            .max_by_key(|p| p.len())
            .unwrap();
        assert!(simulate_route(
            &points,
            &[&paths[0].judge_areas, &paths[1].judge_areas],
            Some(SolverConfig::v3().palm_radius)
        )
        .is_some());
        assert!(s.warnings.iter().any(|w| w.contains("張開同時覆蓋")));
    }
    // V1 仍是一手一條，另一手被 Hold 佔住就無解。
    assert_eq!(run(source, SolverConfig::default()).status, "no_solution");
    // 相隔 90° 的兩條線超出手掌範圍，不能一手完成。
    assert_eq!(
        run("(120){4}1-5[4:1]/3-7[4:1]/6h[1:1],E", SolverConfig::v3()).status,
        "no_solution"
    );
}

#[test]
fn v3_tracking_hand_taps_a_button_whose_outer_area_it_is_touching() {
    // 1-3 的最後判定區是 A3；時長讓正解時刻剛好落在 1.0 秒，那時 3 鍵有 Tap。
    // 另一手按住 7，只有追 Slide 的手可以在完成的同時用 A3 觸發這顆 Tap。
    let source = "(120){4}1-3[#0.61125]/7h[1:1],,3,E";
    let r = run(source, SolverConfig::v3());
    assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
    for s in &r.solutions {
        let slide = s
            .assignments
            .iter()
            .find(|a| a.part == "slide")
            .unwrap()
            .hand;
        let tap = s.assignments.iter().find(|a| a.note_id == "n2").unwrap();
        assert_eq!(tap.hand, slide);
        // 順手點下去，不另外產生動作段。
        assert!(!s
            .left_segments
            .iter()
            .chain(&s.right_segments)
            .any(|g| g.note_id.as_deref() == Some("n2")));
    }
    assert_eq!(run(source, SolverConfig::default()).status, "no_solution");
}

#[test]
fn v3_routes_are_deterministic() {
    let source = "(150){8}1pp5[4:1],3,4*-7[8:1],,2w6[4:1],,,,1-5[4:1]*-4[4:1],E";
    let a = serde_json::to_value(run(source, SolverConfig::v3())).unwrap();
    let b = serde_json::to_value(run(source, SolverConfig::v3())).unwrap();
    assert_eq!(a, b);
}
