use mai_motion_core::*;
fn run(source: &str, solver_config: SolverConfig) -> AnalyzeResponse {
    analyze_chart(AnalyzeRequest {
        request_id: "wifi".into(),
        source: source.into(),
        first_seconds: 0.0,
        solver_config,
    })
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
/// 某隻手在這條 Slide 上的軌跡折線（第一點為接上的位置）。
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

/// V3 WiFi：依三條判定佇列模擬兩手（或單手）的實際軌跡，每條線都要被某隻手完成，
/// 而且完成時刻就是 Slide 指派的結束時間（尾判正解時刻）。
fn assert_wifi_judged(r: &AnalyzeResponse, palm: f64) {
    let chart = r.chart.as_ref().unwrap();
    let path = &chart.paths[0];
    let note = &chart.notes[0];
    let judge = note.motion_start.unwrap()
        + (note.motion_end.unwrap() - note.motion_start.unwrap()) * path.judge_progress;
    let queues: Vec<&[JudgeArea]> = [
        &path.judge_areas,
        &path.branch_judge_areas[0],
        &path.branch_judge_areas[1],
    ]
    .into_iter()
    .map(|q| q.as_slice())
    .collect();
    for solution in &r.solutions {
        continuity(&solution.left_segments);
        continuity(&solution.right_segments);
        assert!(solution.handovers.is_empty());
        let slides: Vec<_> = solution
            .assignments
            .iter()
            .filter(|a| a.part == "slide" && a.note_id == note.id)
            .collect();
        assert!((1..=2).contains(&slides.len()));
        for a in &slides {
            assert!((a.end_seconds - judge).abs() < 1e-9);
            assert_eq!(a.start_seconds, slides[0].start_seconds);
        }
        let mut judged = [false; 3];
        for segments in [&solution.left_segments, &solution.right_segments] {
            let points = traced(segments, &note.id);
            if points.len() < 2 {
                continue;
            }
            for (q, queue) in queues.iter().enumerate() {
                if simulate_route(&points, &[queue], Some(palm)).is_some() {
                    judged[q] = true;
                }
            }
        }
        assert_eq!(judged, [true; 3], "每條 WiFi 線都要依判定佇列完成");
    }
}

#[test]
fn v3_wifi_follows_the_three_judge_queues() {
    for palm in [0.5, 0.2] {
        for start in 1..=8 {
            let end = (start + 3) % 8 + 1;
            let mut c = SolverConfig::v3();
            c.palm_radius = palm;
            let r = run(&format!("(120){{4}}{start}w{end}[4:2],E"), c);
            assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
            assert_wifi_judged(&r, palm);
            if palm < 0.3 {
                // 手掌張不開時單手覆蓋不了三條，必須雙手分工。
                for s in &r.solutions {
                    assert_eq!(
                        s.assignments.iter().filter(|a| a.part == "slide").count(),
                        2
                    );
                }
            }
        }
    }
}

#[test]
fn v3_one_spread_hand_can_play_wifi_while_the_other_holds() {
    let r = run("(120){4}1w5[4:2]/8h[4:8],E", SolverConfig::v3());
    assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
    assert_wifi_judged(&r, 0.5);
    for s in &r.solutions {
        let hold = s
            .assignments
            .iter()
            .find(|a| a.note_id == "n1")
            .unwrap()
            .hand;
        assert!(s
            .assignments
            .iter()
            .filter(|a| a.part == "slide")
            .all(|a| a.hand != hold));
    }
    let mut narrow = SolverConfig::v3();
    narrow.palm_radius = 0.2;
    assert_eq!(
        run("(120){4}1w5[4:2]/8h[4:8],E", narrow).status,
        "no_solution"
    );
}

#[test]
fn wifi_covers_three_synchronous_lines_with_two_continuous_hands() {
    for config in [SolverConfig::default(), SolverConfig::v2()] {
        for start in 1..=8 {
            let end = (start + 3) % 8 + 1;
            let r = run(&format!("(120){{4}}{start}w{end}[4:2],E"), config.clone());
            assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
            let path = &r.chart.as_ref().unwrap().paths[0];
            for solution in &r.solutions {
                continuity(&solution.left_segments);
                continuity(&solution.right_segments);
                let a: Vec<_> = solution
                    .assignments
                    .iter()
                    .filter(|a| a.part == "slide")
                    .collect();
                assert_eq!(a.len(), 2);
                assert_ne!(a[0].hand, a[1].hand);
                assert_eq!(a[0].start_seconds, a[1].start_seconds);
                assert_eq!(a[0].end_seconds, a[1].end_seconds);
                assert!(solution.handovers.is_empty());
                for u in [0.1, 0.5, 0.9, 1.0] {
                    let time = a[0].start_seconds + u * (a[0].end_seconds - a[0].start_seconds);
                    let hands: Vec<Point> = [&solution.left_segments, &solution.right_segments]
                        .iter()
                        .map(|segments| {
                            let segment = segments
                                .iter()
                                .find(|s| {
                                    s.mode == "slide"
                                        && s.start_seconds <= time + 1e-8
                                        && s.end_seconds >= time - 1e-8
                                })
                                .unwrap();
                            let p = &segment.samples;
                            let v = (time - p[0].time_seconds)
                                / (p[1].time_seconds - p[0].time_seconds);
                            p[0].point().lerp(p[1].point(), v)
                        })
                        .collect();
                    let sides: Vec<_> = path
                        .branches
                        .iter()
                        .map(|b| {
                            let p = b.last().unwrap();
                            path.at(0.0).lerp(Point { x: p.x, y: p.y }, u)
                        })
                        .collect();
                    let center = path.at(u);
                    assert!(hands
                        .iter()
                        .any(|h| h.distance(center) <= config.palm_radius + 1e-7));
                    for side in &sides {
                        assert!(hands
                            .iter()
                            .any(|h| h.distance(*side) <= config.palm_radius + 1e-7));
                    }
                    assert!(hands
                        .iter()
                        .any(|h| sides.iter().any(|p| h.distance(*p) < 1e-7)));
                    assert!(hands.iter().any(|h| sides
                        .iter()
                        .any(|p| h.distance(center.lerp(*p, 0.5)) < 1e-7)));
                }
            }
        }
    }
}
#[test]
fn wifi_requires_both_hands_and_sufficient_palm_width() {
    // V3 依判定佇列，張開的單手也能完成 WiFi，見 v3_one_spread_hand_can_play_wifi_while_the_other_holds。
    for mut c in [SolverConfig::default(), SolverConfig::v2()] {
        assert_eq!(
            run("(120){4}1w5[4:2]/8h[4:8],E", c.clone()).status,
            "no_solution"
        );
        c.palm_radius = 0.2;
        assert_eq!(run("(120){4}1w5[4:2],E", c).status, "no_solution");
    }
}
#[test]
fn uncontested_v3_slides_keep_their_original_owner() {
    for source in [
        "(120){4}1-5[1:2],E",
        "(120){4}2p6[1:2],E",
        "(120){4}1-5[4:1]/5-1[4:1],E",
    ] {
        let mut c = SolverConfig::v3();
        c.handover_willingness = 100.0;
        let r = run(source, c);
        assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
        assert!(r.solutions.iter().all(|s| s.handovers.is_empty()));
    }
}
#[test]
fn wifi_hands_can_brush_a_touch_without_leaving_the_fan() {
    let r = run("(120){4}1w5[4:2],{8},C,E", SolverConfig::v3());
    assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
    for s in &r.solutions {
        continuity(&s.left_segments);
        continuity(&s.right_segments);
        let hands = s.assignments.iter().filter(|a| a.part == "slide").count();
        assert!((1..=2).contains(&hands));
        let touch = r
            .chart
            .as_ref()
            .unwrap()
            .notes
            .iter()
            .find(|n| n.kind == "touch")
            .unwrap();
        assert!(s.assignments.iter().any(|a| a.note_id == touch.id));
        // 兩手都在 WiFi 上時，Touch 只能由經過的手掌順帶完成；單手 WiFi 時另一手可直接去按。
        if hands == 2 {
            assert!(!s
                .left_segments
                .iter()
                .chain(&s.right_segments)
                .any(|seg| seg.note_id.as_ref() == Some(&touch.id)));
        }
    }
}

#[test]
fn wifi_is_deterministic_and_late_pickup_stays_synchronous() {
    // Heads at 0.5 occupy both hands until 0.53; WiFi must start both at 0.53.
    // 手掌 0.2 張不開，必須雙手分工，才能檢查兩手同步晚接。
    let source = "(120){4}1w5[4:2],1/8,E";
    let mut c = SolverConfig::v3();
    c.top_k = 1;
    c.palm_radius = 0.2;
    let r = run(source, c.clone());
    assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
    let a: Vec<_> = r.solutions[0]
        .assignments
        .iter()
        .filter(|a| a.part == "slide")
        .collect();
    assert_eq!(a.len(), 2);
    assert!(a[0].start_seconds > 0.5);
    assert_eq!(a[0].start_seconds, a[1].start_seconds);
    assert_eq!(
        serde_json::to_value(&r).unwrap(),
        serde_json::to_value(run(source, c)).unwrap()
    );
}

#[test]
fn mixed_wifi_path_is_reported_with_source_location_instead_of_dropping_sides() {
    let r = run("(120){4}1w5-1[4:2],E", SolverConfig::v3());
    assert_eq!(r.status, "invalid");
    assert!(r
        .diagnostics
        .iter()
        .any(|d| d.source_span.is_some() && d.message.contains("WiFi")));
}
