use mai_motion_core::*;

fn analyze(source: &str) -> AnalyzeResponse {
    analyze_chart(AnalyzeRequest {
        request_id: "test".into(),
        source: source.into(),
        first_seconds: 0.0,
        solver_config: SolverConfig::default(),
    })
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-7, "{a} != {b}");
}

#[test]
fn timing_and_bpm_changes() {
    let c = parse_chart("(120){4}1,2,(240)3,4,E", 0.2).unwrap();
    for (n, t) in c.notes.iter().zip([0.2, 0.7, 1.2, 1.45]) {
        near(n.time_seconds, t);
    }
    near(c.duration_seconds, 1.7);
}
#[test]
fn slide_wait_is_separate_from_head() {
    let c = parse_chart("(120){4}1-5[4:2],E", 0.0).unwrap();
    near(c.notes[0].motion_start.unwrap(), 0.5);
    near(c.notes[0].motion_end.unwrap(), 1.5);
    let c = parse_chart("(120){4}1-5[2##1.5],E", 0.0).unwrap();
    near(c.notes[0].motion_start.unwrap(), 2.0);
    near(c.notes[0].end_seconds, 3.5);
}
#[test]
fn absolute_step_and_hold_duration() {
    let c = parse_chart("{#0.35}1h[#0.2],2,E", 0.0).unwrap();
    near(c.notes[1].time_seconds, 0.35);
    near(c.notes[0].end_seconds, 0.2);
    let c = parse_chart("(120){4}1h[240#4:2],E", 0.0).unwrap();
    near(c.notes[0].end_seconds, 0.5);
}
#[test]
fn invalid_input_and_unicode_location() {
    for s in [
        "(0){4}1,E",
        "(120){0}1,E",
        "(120){4}1,",
        "(120){4}/1,E",
        "(120){4}1/,E",
        "(120){4}1h[0:1],E",
        "(120){4}1-2[4:1],E",
        "(120){4}1^5[4:1],E",
    ] {
        assert!(parse_chart(s, 0.0).is_err(), "{s}");
    }
    let err = parse_chart("(120){4}\n　B1,E", 0.0).unwrap_err();
    let span = err.source_span.unwrap();
    assert_eq!(span.line, 2);
    assert_eq!(span.column, 2);
    assert_eq!(&"(120){4}\n　B1,E"[span.start..span.end], "B");
    assert_eq!(analyze("(120){4}C,E").status, "unsupported");
}
#[test]
fn simultaneous_order_does_not_change_cost() {
    let a = analyze("(120){4}1/8,2/7,E");
    let b = analyze("(120){4}8/1,7/2,E");
    assert_eq!(a.status, "ok");
    assert_eq!(b.status, "ok");
    near(a.solutions[0].total_cost, b.solutions[0].total_cost);
}
#[test]
fn no_third_hand_is_invented() {
    assert_eq!(analyze("(120){4}1/4/7,E").status, "no_solution");
}
#[test]
fn hold_reserves_hand() {
    let r = analyze("(120){4}8h[4:4]/1,2,3,E");
    assert_eq!(r.status, "ok");
    for s in &r.solutions {
        let owner = s
            .assignments
            .iter()
            .find(|a| a.note_id == "n0")
            .unwrap()
            .hand;
        assert!(s
            .assignments
            .iter()
            .filter(|a| a.note_id != "n0")
            .all(|a| a.hand != owner));
    }
}

fn verify_segments(segments: &[MotionSegment]) {
    for pair in segments.windows(2) {
        near(pair[0].end_seconds, pair[1].start_seconds);
        let a = pair[0].samples.last().unwrap();
        let b = &pair[1].samples[0];
        near(a.x, b.x);
        near(a.y, b.y);
    }
    for s in segments {
        assert!(s.end_seconds >= s.start_seconds);
        for p in s.samples.windows(2) {
            assert!(p[1].time_seconds >= p[0].time_seconds);
        }
    }
}
#[test]
fn trajectories_are_continuous_and_deterministic() {
    let source = "(120){4}1-5[4:3],8,7,6,E";
    let a = analyze(source);
    let b = analyze(source);
    assert_eq!(a.status, "ok");
    assert_eq!(
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&b).unwrap()
    );
    for s in &a.solutions {
        verify_segments(&s.left_segments);
        verify_segments(&s.right_segments);
        near(s.total_cost, s.cost_breakdown.total());
    }
}
#[test]
fn slide_geometry_follows_circle_and_expected_direction() {
    let c = parse_chart("(120){4}1>5[4:1],5>1[4:1],1^8[4:1],E", 0.0).unwrap();
    assert!(c.paths[0].at(0.5).x > 0.0);
    assert!(c.paths[1].at(0.5).x > 0.0);
    for p in &c.paths {
        for sample in &p.samples {
            near(sample.x.hypot(sample.y), 1.0);
        }
    }
}
#[test]
fn config_errors_are_reported_not_panics() {
    let c = SolverConfig {
        beam_width: 0,
        ..SolverConfig::default()
    };
    let r = analyze_chart(AnalyzeRequest {
        request_id: "bad".into(),
        source: "(120){4}1,E".into(),
        first_seconds: 0.0,
        solver_config: c,
    });
    assert_eq!(r.status, "invalid");
    assert_eq!(r.diagnostics[0].code, "invalid_config");
}

fn pose(segments: &[MotionSegment], t: f64) -> Point {
    let s = segments
        .iter()
        .find(|s| s.start_seconds - 1e-8 <= t && s.end_seconds + 1e-8 >= t)
        .unwrap();
    let pair = s
        .samples
        .windows(2)
        .find(|p| p[0].time_seconds - 1e-8 <= t && p[1].time_seconds + 1e-8 >= t)
        .unwrap();
    let u = (t - pair[0].time_seconds) / (pair[1].time_seconds - pair[0].time_seconds);
    pair[0].point().lerp(pair[1].point(), u)
}

#[test]
fn handover_has_reachable_overlap_and_lower_cost() {
    let config = SolverConfig {
        handover_weight: 0.05,
        side_weight: 12.0,
        ..SolverConfig::default()
    };
    let mut request = AnalyzeRequest {
        request_id: "handover".into(),
        source: "(120){4}8>4[1##3],,,,,,,8,E".into(),
        first_seconds: 0.0,
        solver_config: config,
    };
    let r = analyze_chart(request.clone());
    assert_eq!(r.status, "ok");
    let s = &r.solutions[0];
    assert!(!s.handovers.is_empty());
    verify_segments(&s.left_segments);
    verify_segments(&s.right_segments);
    for h in &s.handovers {
        near(
            h.end_seconds - h.start_seconds,
            request.solver_config.handover_seconds,
        );
        for i in 0..=10 {
            let t = h.start_seconds + (h.end_seconds - h.start_seconds) * i as f64 / 10.0;
            let l = pose(&s.left_segments, t);
            let r = pose(&s.right_segments, t);
            assert!(l.distance(r) < 1e-6);
        }
    }
    request.solver_config.allow_handover = false;
    let other = analyze_chart(request);
    assert_eq!(other.status, "ok");
    assert!(other.solutions[0].handovers.is_empty());
    assert!(s.total_cost < other.solutions[0].total_cost);
}

#[test]
fn busy_other_hand_prevents_handover() {
    let r = analyze("(120){4}1-5[4:2]/8h[4:4],E");
    assert_eq!(r.status, "ok");
    assert!(r.solutions.iter().all(|s| s.handovers.is_empty()));
}

#[test]
fn checkpoint_refinement_does_not_scale_cost_with_count() {
    let mut req = AnalyzeRequest {
        request_id: "resolution".into(),
        source: "(120){4}1>5[4:2],E".into(),
        first_seconds: 0.0,
        solver_config: SolverConfig::default(),
    };
    req.solver_config.allow_handover = false;
    let a = analyze_chart(req.clone());
    req.solver_config.checkpoint_seconds = 0.025;
    req.solver_config.handover_seconds = 0.02;
    let b = analyze_chart(req);
    assert_eq!(a.status, "ok");
    assert_eq!(b.status, "ok");
    assert!((a.solutions[0].total_cost - b.solutions[0].total_cost).abs() < 0.001);
}
