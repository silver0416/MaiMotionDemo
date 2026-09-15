use mai_motion_core::*;

fn analyze(source: &str) -> AnalyzeResponse {
    analyze_chart(AnalyzeRequest {
        request_id: "test".into(),
        source: source.into(),
        first_seconds: 0.0,
        solver_config: SolverConfig::default(),
    })
}
fn chart(source: &str) -> Chart {
    parse_chart(source, 0.0).unwrap().chart
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-7, "{a} != {b}");
}

#[test]
fn timing_and_bpm_changes() {
    let c = parse_chart("(120){4}1,2,(240)3,4,E", 0.2).unwrap().chart;
    for (n, t) in c.notes.iter().zip([0.2, 0.7, 1.2, 1.45]) {
        near(n.time_seconds, t);
    }
    near(c.duration_seconds, 1.7);
}
#[test]
fn slide_wait_is_separate_from_head() {
    let c = chart("(120){4}1-5[4:2],E");
    near(c.notes[0].motion_start.unwrap(), 0.5);
    near(c.notes[0].motion_end.unwrap(), 1.5);
    let c = chart("(120){4}1-5[2##1.5],E");
    near(c.notes[0].motion_start.unwrap(), 2.0);
    near(c.notes[0].end_seconds, 3.5);
}
#[test]
fn absolute_step_and_hold_duration() {
    let c = chart("{#0.35}1h[#0.2],2,E");
    near(c.notes[1].time_seconds, 0.35);
    near(c.notes[0].end_seconds, 0.2);
    let c = chart("(120){4}1h[240#4:2],E");
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
        "(120){4}1V83[4:1],E",
        "(120){4}1s3[4:1],E",
        "(120){4}1w3[4:1],E",
        "(120){4}1-5,E",
        "(120){4}A9,E",
        "(120){4}1k5[4:1],E",
        "(120){4}1,E,2,E",
    ] {
        assert!(parse_chart(s, 0.0).is_err(), "{s}");
    }
    let err = parse_chart("(120){4}\n　K1,E", 0.0).unwrap_err();
    let span = err.source_span.unwrap();
    assert_eq!(span.line, 2);
    assert_eq!(span.column, 2);
    assert_eq!(&"(120){4}\n　K1,E"[span.start..span.end], "K");
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
    let c = chart("(120){4}1>5[4:1],5>1[4:1],1^8[4:1],E");
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

#[test]
fn touch_areas_have_distinct_positions() {
    let c = chart("(120){4}A1/B1/C/D1/E1,Ch[4:1],E");
    let kinds: Vec<&str> = c.notes.iter().map(|n| n.kind.as_str()).collect();
    assert_eq!(
        kinds,
        ["touch", "touch", "touch", "touch", "touch", "touchHold"]
    );
    let areas: Vec<Option<&str>> = c.notes.iter().map(|n| n.touch_area.as_deref()).collect();
    assert_eq!(
        areas,
        [
            Some("A"),
            Some("B"),
            Some("C"),
            Some("D"),
            Some("E"),
            Some("C")
        ]
    );
    near(c.notes[2].position.x, 0.0);
    near(c.notes[2].position.y, 0.0);
    // A 在外圈、B 在內圈、D/E 相對旋轉半格，四者互不重疊。
    for i in 0..5 {
        for j in i + 1..5 {
            assert!(c.notes[i].position.distance(c.notes[j].position) > 1e-3);
        }
    }
    near(c.notes[5].end_seconds, 1.0);
}

#[test]
fn every_slide_shape_starts_and_ends_on_its_buttons() {
    for source in [
        "(120){4}1-5[4:1],E",
        "(120){4}1^3[4:1],E",
        "(120){4}1<5[4:1],E",
        "(120){4}1>5[4:1],E",
        "(120){4}1v4[4:1],E",
        "(120){4}1V73[4:1],E",
        "(120){4}1p5[4:1],E",
        "(120){4}1q5[4:1],E",
        "(120){4}1pp5[4:1],E",
        "(120){4}1qq5[4:1],E",
        "(120){4}1s5[4:1],E",
        "(120){4}1z5[4:1],E",
        "(120){4}1w5[4:1],E",
    ] {
        let c = chart(source);
        let path = &c.paths[0];
        assert!(
            path.at(0.0).distance(c.notes[0].position) < 1e-9,
            "{source}"
        );
        let end = mai_motion_core::Point {
            x: path.samples.last().unwrap().x,
            y: path.samples.last().unwrap().y,
        };
        assert!(path.at(1.0).distance(end) < 1e-9, "{source}");
        for w in path.samples.windows(2) {
            assert!(w[1].u >= w[0].u, "{source}");
            assert!(w[0].x.hypot(w[0].y) <= 1.0 + 1e-9, "{source}");
        }
        near(path.samples[0].u, 0.0);
        near(path.samples.last().unwrap().u, 1.0);
    }
}

#[test]
fn wifi_carries_side_branches_and_others_do_not() {
    let c = chart("(120){4}1w5[4:1],1-5[4:1],E");
    assert_eq!(c.paths[0].shape, "w");
    assert_eq!(c.paths[0].branches.len(), 2);
    assert!(c.paths[1].branches.is_empty());
}

#[test]
fn chained_slide_shares_or_splits_its_duration() {
    // 單一長度：整條連續軌道共用一段時間，只有一顆音符。
    let c = chart("(120){4}1-3-5[4:1],E");
    assert_eq!(c.notes.len(), 1);
    assert_eq!(c.paths.len(), 1);
    assert_eq!(c.paths[0].shape, "--");
    near(
        c.notes[0].motion_end.unwrap() - c.notes[0].motion_start.unwrap(),
        0.5,
    );

    // 各自長度：拆成前後接續的兩段，第二段沒有起點觸碰。
    let c = chart("(120){4}1-3[4:1]-5[4:1],E");
    assert_eq!(c.notes.len(), 2);
    assert!(c.notes[0].has_head);
    assert!(!c.notes[1].has_head);
    near(
        c.notes[0].motion_end.unwrap(),
        c.notes[1].motion_start.unwrap(),
    );
}

#[test]
fn star_with_two_slides_keeps_one_head() {
    let c = chart("(120){4}1-3[4:1]*-7[4:1],E");
    assert_eq!(c.notes.len(), 2);
    assert!(c.notes[0].has_head);
    assert!(!c.notes[1].has_head);
    near(
        c.notes[0].motion_start.unwrap(),
        c.notes[1].motion_start.unwrap(),
    );
}

#[test]
fn headless_slide_creates_no_contact_task() {
    let with_head = analyze("(120){4}1-5[4:1],E");
    let without = analyze("(120){4}1?-5[4:1],E");
    assert_eq!(with_head.status, "ok");
    assert_eq!(without.status, "ok");
    assert!(with_head.solutions[0]
        .assignments
        .iter()
        .any(|a| a.part == "head"));
    assert!(!without.solutions[0]
        .assignments
        .iter()
        .any(|a| a.part == "head"));
}

#[test]
fn modifiers_are_recorded_not_dropped() {
    let c = chart("(120){4}1b,2x,3bx,4$$,A1f,5-1[4:1]b,E");
    assert!(c.notes[0].modifiers.break_note);
    assert!(c.notes[1].modifiers.ex);
    assert!(c.notes[2].modifiers.break_note && c.notes[2].modifiers.ex);
    assert!(c.notes[3].modifiers.star && c.notes[3].modifiers.spin_star);
    assert!(c.notes[4].modifiers.fireworks);
    assert!(c.notes[5].modifiers.break_slide);
}

#[test]
fn pseudo_each_offsets_notes_slightly() {
    let c = chart("(120){4}1`5,2,E");
    assert!(c.notes[1].time_seconds > c.notes[0].time_seconds);
    near(
        c.notes[1].time_seconds - c.notes[0].time_seconds,
        0.5 / 96.0,
    );
    // 逗號之後位移歸零。
    near(c.notes[2].time_seconds, 0.5);
}

#[test]
fn comments_do_not_shift_error_positions() {
    let source = "(120){4}1,||說明\n2,1-2[4:1],E";
    let err = parse_chart(source, 0.0).unwrap_err();
    let span = err.source_span.unwrap();
    assert_eq!(span.line, 2);
    assert_eq!(&source[span.start..span.end], "1-2[4:1]");
}

#[test]
fn maidata_file_selects_the_hardest_chart() {
    let source = "&title=demo\n&first=1.5\n&inote_2=(120){4}1,2,E\n&inote_5=(120){4}1,2,3,E\n";
    let parsed = parse_chart(source, 0.0).unwrap();
    assert_eq!(parsed.chart.notes.len(), 3);
    assert!(parsed.notices.iter().all(|d| d.severity == "info"));
    assert!(parsed.notices.iter().any(|d| d.code == "maidata_chart"));
    assert!(parsed.notices.iter().any(|d| d.code == "maidata_first"));
    // 錯誤位置仍指回原檔案。
    let broken = "&inote_1=(120){4}1-2[4:1],E\n";
    let err = parse_chart(broken, 0.0).unwrap_err();
    let span = err.source_span.unwrap();
    assert_eq!(&broken[span.start..span.end], "1-2[4:1]");
}

#[test]
fn one_hand_covers_simultaneous_contacts_at_the_same_point() {
    // Slide 的移動起點上同時有 Break Tap：同一隻手一次接觸即可滿足兩者。
    let r = analyze("(240){8}4^1[4:1]/8^5[4:1],,4b/8b,,,,,,E");
    assert_eq!(r.status, "ok");
    let s = &r.solutions[0];
    for id in ["n0", "n1", "n2", "n3"] {
        assert!(s.assignments.iter().any(|a| a.note_id == id), "{id}");
    }
    // 同時接觸不會多產生一段軌跡，因此兩手的動作段仍然首尾相接、不重疊。
    verify_segments(&s.left_segments);
    verify_segments(&s.right_segments);
    near(s.total_cost, s.cost_breakdown.total());
    // 位置不同的三顆同時音仍然無解，不會因此放寬成三隻手。
    assert_eq!(analyze("(120){4}1/4/7,E").status, "no_solution");
}
