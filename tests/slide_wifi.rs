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
#[test]
fn wifi_covers_three_synchronous_lines_with_two_continuous_hands() {
    for config in [
        SolverConfig::default(),
        SolverConfig::v2(),
        SolverConfig::v3(),
    ] {
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
    for mut c in [
        SolverConfig::default(),
        SolverConfig::v2(),
        SolverConfig::v3(),
    ] {
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
        assert_eq!(
            s.assignments.iter().filter(|a| a.part == "slide").count(),
            2
        );
        let touch = r
            .chart
            .as_ref()
            .unwrap()
            .notes
            .iter()
            .find(|n| n.kind == "touch")
            .unwrap();
        assert!(s.assignments.iter().any(|a| a.note_id == touch.id));
        assert!(!s
            .left_segments
            .iter()
            .chain(&s.right_segments)
            .any(|seg| seg.note_id.as_ref() == Some(&touch.id)));
    }
}

#[test]
fn wifi_is_deterministic_and_late_pickup_stays_synchronous() {
    // Heads at 0.5 occupy both hands until 0.53; WiFi must start both at 0.53.
    let source = "(120){4}1w5[4:2],1/8,E";
    let mut c = SolverConfig::v3();
    c.top_k = 1;
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
