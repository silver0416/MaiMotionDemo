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
fn majdata_accepts_sub_millisecond_1280th_holds() {
    let c = chart("(200){64}1hx[1280:1]/6x/B1/C,E");
    let hold = &c.notes[0];
    near(hold.end_seconds - hold.time_seconds, 0.0009375);
    assert!(hold.modifiers.ex);
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
fn palm_covers_center_and_inner_touches_while_other_hand_taps() {
    let r = analyze("(120){4}C/B1/E1/7,E");
    assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
    let s = &r.solutions[0];
    let palm = s.palm_placements.first().unwrap();
    near(palm.radius, 0.5);
    assert_eq!(palm.covered_note_ids.len(), 3);
    assert!(palm.covered_note_ids.iter().all(|id| {
        r.chart
            .as_ref()
            .unwrap()
            .notes
            .iter()
            .find(|n| &n.id == id)
            .is_some_and(|n| {
                matches!(n.kind.as_str(), "touch" | "touchHold")
                    && n.position.distance(palm.center) <= palm.radius + 1e-8
            })
    }));
    let tap = r
        .chart
        .as_ref()
        .unwrap()
        .notes
        .iter()
        .find(|n| n.kind == "tap")
        .unwrap();
    assert!(!palm.covered_note_ids.contains(&tap.id));
    assert_ne!(
        s.assignments
            .iter()
            .find(|a| a.note_id == tap.id)
            .unwrap()
            .hand,
        palm.hand
    );
    let segments = if palm.hand == Hand::L {
        &s.left_segments
    } else {
        &s.right_segments
    };
    assert!(segments.iter().any(|seg| seg.mode == "palm"
        && seg
            .samples
            .iter()
            .all(|p| p.point().distance(palm.center) < 1e-8)));
    verify_segments(&s.left_segments);
    verify_segments(&s.right_segments);
    near(s.total_cost, s.cost_breakdown.total());
}
#[test]
fn palm_radius_can_be_disabled_or_reduced() {
    let source = "(120){4}C/B1/E1/7,E";
    for radius in [0.0, 0.15] {
        let r = analyze_chart(AnalyzeRequest {
            request_id: "small-palm".into(),
            source: source.into(),
            first_seconds: 0.0,
            solver_config: SolverConfig {
                palm_radius: radius,
                ..SolverConfig::default()
            },
        });
        assert_eq!(r.status, "no_solution", "radius={radius}");
    }
    assert_eq!(analyze("(120){4}A1/A5/1,E").status, "no_solution");
    assert_eq!(analyze("(120){4}1/2/3,E").status, "no_solution");
    let separated = analyze("(120){4}A1/A5,E");
    assert_eq!(separated.status, "ok");
    assert!(separated.solutions[0].palm_placements.is_empty());
    let wide = analyze_chart(AnalyzeRequest {
        request_id: "wide-palm".into(),
        source: "(120){4}A1/A5/1,E".into(),
        first_seconds: 0.0,
        solver_config: SolverConfig {
            palm_radius: 1.0,
            ..SolverConfig::default()
        },
    });
    assert_eq!(wide.status, "ok", "{:?}", wide.diagnostics);
    let invalid = analyze_chart(AnalyzeRequest {
        request_id: "invalid-palm".into(),
        source: "(120){4}C,E".into(),
        first_seconds: 0.0,
        solver_config: SolverConfig {
            palm_radius: 1.1,
            ..SolverConfig::default()
        },
    });
    assert_eq!(invalid.status, "invalid");
    assert_eq!(invalid.diagnostics[0].code, "invalid_config");
    let same_point = analyze("(120){4}C/C1,E");
    assert_eq!(same_point.status, "ok");
    assert!(same_point.solutions[0].palm_placements.is_empty());
}
#[test]
fn touch_hold_in_a_palm_reserves_the_hand_until_release() {
    let feasible = analyze("(120){4}Ch[4:4]/B1/E1/7,8,E");
    assert_eq!(feasible.status, "ok", "{:?}", feasible.diagnostics);
    assert!(feasible.solutions[0]
        .palm_placements
        .iter()
        .any(|p| { p.covered_note_ids.len() == 3 && p.end_seconds >= 2.0 - 1e-8 }));
    let blocked = analyze("(120){4}Ch[4:4]/B1/E1/7,8/2,E");
    assert_eq!(blocked.status, "no_solution");
}

#[test]
fn active_center_touch_hold_can_expand_to_cover_later_b_touches() {
    let source = "(190){4}1h[4:15],,,Ch[1:3],,,,,B1,B8,B2,B7,B3,B6,B4,B5,E";
    let r = analyze(source);
    assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
    let solution = &r.solutions[0];
    let chart = r.chart.as_ref().unwrap();
    let center = &chart.notes[1];
    let palm = solution
        .palm_placements
        .iter()
        .find(|p| p.covered_note_ids.contains(&center.id))
        .expect("中央 Touch Hold 應擴展成手掌覆蓋");
    assert_eq!(palm.covered_note_ids.len(), 9);
    assert!(palm.end_seconds >= chart.notes.last().unwrap().end_seconds - 1e-8);
    let owner = solution
        .assignments
        .iter()
        .find(|a| a.note_id == center.id)
        .unwrap()
        .hand;
    assert!(chart.notes[1..].iter().all(|note| {
        solution
            .assignments
            .iter()
            .any(|a| a.note_id == note.id && a.hand == owner)
    }));
    verify_segments(&solution.left_segments);
    verify_segments(&solution.right_segments);

    let disabled = analyze_chart(AnalyzeRequest {
        request_id: "sequential-palm-disabled".into(),
        source: source.into(),
        first_seconds: 0.0,
        solver_config: SolverConfig {
            palm_radius: 0.0,
            ..SolverConfig::default()
        },
    });
    assert_eq!(disabled.status, "no_solution");
    assert_eq!(
        analyze("(120){4}1h[4:8],Ch[4:4],A1,E").status,
        "no_solution",
        "超出手掌半徑的後續 Touch 不可合併"
    );
}
#[test]
fn both_hands_can_cover_separate_touch_clusters() {
    let r = analyze("(120){4}A1/B1/A5/B5,E");
    assert_eq!(r.status, "ok", "{:?}", r.diagnostics);
    let placements = &r.solutions[0].palm_placements;
    assert_eq!(placements.len(), 2);
    assert_ne!(placements[0].hand, placements[1].hand);
    assert!(placements.iter().all(|p| p.covered_note_ids.len() == 2));
}
#[test]
fn palm_search_is_deterministic_and_independent_of_simultaneous_order() {
    let a = analyze("(120){4}C/B1/E1/7,E");
    let b = analyze("(120){4}7/E1/B1/C,E");
    assert_eq!(a.status, "ok");
    assert_eq!(b.status, "ok");
    near(a.solutions[0].total_cost, b.solutions[0].total_cost);
    let again = analyze("(120){4}C/B1/E1/7,E");
    assert_eq!(
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&again).unwrap()
    );
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
    for h in s.handovers.iter().filter(|h| !h.swap) {
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
    assert_eq!(c.touch_sensors.len(), 33);
    for note in &c.notes {
        let sensor = c
            .touch_sensors
            .iter()
            .find(|sensor| {
                Some(sensor.area.as_str()) == note.touch_area.as_deref()
                    && sensor.index == note.button
            })
            .unwrap();
        near(sensor.position.x, note.position.x);
        near(sensor.position.y, note.position.y);
    }
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
fn majdata_chart_keeps_motion_timing_through_hs_and_pre_length_break() {
    let source = "&title=demo\n&inote_5=(120){4}<HS*1.5>1`5,2>6qq4b[4:1],3";
    let parsed = parse_chart(source, 0.0).unwrap();
    assert_eq!(parsed.chart.notes.len(), 4);
    near(parsed.chart.notes[1].time_seconds, 0.5 / 32.0);
    near(parsed.chart.notes[2].time_seconds, 0.5);
    assert!(parsed.chart.notes[2].modifiers.break_slide);
    assert!(parsed.notices.iter().any(|d| d.code == "majdata_hs"));
    assert!(parsed.notices.iter().any(|d| d.code == "majdata_end"));
    assert!(parsed
        .notices
        .iter()
        .any(|d| d.code == "majdata_pseudo_each"));
    assert!(parsed.notices.iter().all(|d| d.severity == "info"));
}

#[test]
fn majdata_diagnostics_reach_analyze_response() {
    let response = analyze("&inote_5=(120){4}<HS*1.5>1,2");
    assert_eq!(response.status, "ok");
    assert_eq!(response.chart.unwrap().notes.len(), 2);
    assert!(response.diagnostics.iter().any(|d| d.code == "majdata_hs"));
    assert!(response.diagnostics.iter().any(|d| d.code == "majdata_end"));
}

#[test]
fn majdata_hs_errors_keep_original_source_positions() {
    let source = "&inote_5=(120){4}\n<HS*abc>1,E";
    let err = parse_chart(source, 0.0).unwrap_err();
    let span = err.source_span.unwrap();
    assert_eq!(span.line, 2);
    assert_eq!(&source[span.start..span.end], "<HS*abc>");
    assert!(parse_chart("(120){4}<HS*0>1,E", 0.0).is_ok());
    assert!(parse_chart("(120){4}<HS*1E3>1,E", 0.0).is_ok());
    let source = "(120){4}<HS*1.5,1,E";
    let err = parse_chart(source, 0.0).unwrap_err();
    assert!(err.message.contains("缺少 >"));
    assert_eq!(err.source_span.unwrap().column, 9);
    assert!(parse_chart("(120){4}1,2", 0.0).is_err());
    assert!(parse_chart("(120){4}1,||<HS*1>\n2", 0.0).is_err());
    assert!(parse_chart("&inote_5=(120){4}1/", 0.0).is_err());
    assert!(parse_chart("&inote_5=(120){4}1`", 0.0).is_err());
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

#[test]
fn a_hand_may_pick_up_a_slide_late_when_the_start_is_contested() {
    // 起點拍擊與移動之間相隔一拍，移動開始的瞬間另外安排了兩顆音符：
    // 兩隻手都被佔用，手必須稍晚才接上軌道，再於原定終點前走完整條路徑。
    let r = analyze("(120){4}1-5[4:1],3/7,E");
    assert_eq!(r.status, "ok");
    let s = &r.solutions[0];
    let slide: Vec<_> = s
        .assignments
        .iter()
        .filter(|a| a.note_id == "n0" && a.part == "slide")
        .collect();
    assert!(!slide.is_empty());
    let pickup = slide
        .iter()
        .map(|a| a.start_seconds)
        .fold(f64::MAX, f64::min);
    let chart = chart("(120){4}1-5[4:1],3/7,E");
    let motion_start = chart.notes[0].motion_start.unwrap();
    let motion_end = chart.notes[0].motion_end.unwrap();
    assert!(pickup > motion_start + 1e-9, "應該晚於移動開始");
    assert!(pickup <= motion_start + SolverConfig::default().slide_pickup_seconds + 1e-9);
    // 晚接之後仍然走完整條路徑，而且在原定終點前結束。
    let last = slide.iter().map(|a| a.end_seconds).fold(0.0, f64::max);
    near(last, motion_end);
    let owner = slide[0].hand;
    let path = &chart.paths[0];
    let track = if owner == Hand::L {
        &s.left_segments
    } else {
        &s.right_segments
    };
    let traced: Vec<_> = track
        .iter()
        .filter(|g| g.note_id.as_deref() == Some("n0"))
        .collect();
    let first = traced[0].samples[0].point();
    let end = traced[traced.len() - 1].samples.last().unwrap().point();
    assert!(first.distance(path.at(0.0)) < 1e-9, "從路徑起點開始");
    assert!(end.distance(path.at(1.0)) < 1e-9, "走到路徑終點");
    verify_segments(&s.left_segments);
    verify_segments(&s.right_segments);
    near(s.total_cost, s.cost_breakdown.total());
}

#[test]
fn an_uncontested_slide_is_picked_up_on_time() {
    let r = analyze("(120){4}1-5[4:3],8,7,6,E");
    assert_eq!(r.status, "ok");
    let chart = chart("(120){4}1-5[4:3],8,7,6,E");
    // 沒有衝突時，最佳方案不應該無故延後接上（其他候選可以是晚接的變化）。
    let pickup = r.solutions[0]
        .assignments
        .iter()
        .filter(|a| a.note_id == "n0" && a.part == "slide")
        .map(|a| a.start_seconds)
        .fold(f64::MAX, f64::min);
    near(pickup, chart.notes[0].motion_start.unwrap());
}

fn hand_sequence(response: &AnalyzeResponse) -> String {
    let solution = &response.solutions[0];
    response
        .chart
        .as_ref()
        .unwrap()
        .notes
        .iter()
        .map(|n| {
            match solution
                .assignments
                .iter()
                .find(|a| a.note_id == n.id)
                .map(|a| a.hand)
            {
                Some(Hand::L) => 'L',
                Some(Hand::R) => 'R',
                None => '?',
            }
        })
        .collect()
}

#[test]
fn fast_adjacent_run_is_brushed_with_one_hand() {
    // 相鄰鍵位的高速連擊：真實打法是手不抬起、貼著面板滑過去，而不是雙手交替。
    let source = "(240){24}8,7,6,5,4,3,E";
    let r = analyze(source);
    assert_eq!(r.status, "ok");
    let hands = hand_sequence(&r);
    let switches = hands.as_bytes().windows(2).filter(|w| w[0] != w[1]).count();
    assert!(switches <= 1, "應該單手刷過去，實際為 {hands}");
    let s = &r.solutions[0];
    // 手沒有離開面板，所以不算重新擊打。
    near(s.cost_breakdown.repetition, 0.0);
    assert!(
        s.left_segments
            .iter()
            .chain(&s.right_segments)
            .any(|g| g.mode == "glide"),
        "軌跡應該出現滑移段"
    );
    verify_segments(&s.left_segments);
    verify_segments(&s.right_segments);
    near(s.total_cost, s.cost_breakdown.total());
}

#[test]
fn glide_can_be_switched_off() {
    // 關掉滑移就回到原本的模型：停留後衝刺，於是偏好雙手交替。
    let config = SolverConfig {
        glide_distance: 0.0,
        ..SolverConfig::default()
    };
    let r = analyze_chart(AnalyzeRequest {
        request_id: "no-glide".into(),
        source: "(240){24}8,7,6,5,4,3,E".into(),
        first_seconds: 0.0,
        solver_config: config,
    });
    assert_eq!(r.status, "ok");
    let hands = hand_sequence(&r);
    let switches = hands.as_bytes().windows(2).filter(|w| w[0] != w[1]).count();
    assert!(switches > 1, "關閉滑移後應該回到交替，實際為 {hands}");
    assert!(r.solutions[0]
        .left_segments
        .iter()
        .chain(&r.solutions[0].right_segments)
        .all(|g| g.mode != "glide"));
}

#[test]
fn repeating_the_same_button_still_counts_as_restriking() {
    // 同一個點連打必須抬手重按，不是滑移：連打成本要照付。
    let r = analyze("(240){16}5,5,5,5,5,5,E");
    assert_eq!(r.status, "ok");
    assert!(r.solutions[0].cost_breakdown.repetition > 0.0);
    assert!(r.solutions[0]
        .left_segments
        .iter()
        .chain(&r.solutions[0].right_segments)
        .all(|g| g.mode != "glide"));
}

#[test]
fn glide_does_not_reach_across_non_adjacent_buttons() {
    // 隔兩鍵（距離 1.414）超過預設滑移距離，仍然算抬手重按。
    let r = analyze("(240){16}1,3,1,3,E");
    assert_eq!(r.status, "ok");
    for s in &r.solutions {
        assert!(s
            .left_segments
            .iter()
            .chain(&s.right_segments)
            .all(|g| g.mode != "glide"));
    }
}

/// 兩手在 time 當下的距離。
fn hand_gap(solution: &Solution, time: f64) -> f64 {
    pose(&solution.left_segments, time).distance(pose(&solution.right_segments, time))
}

#[test]
fn hands_swap_destinations_where_crossing_slides_meet() {
    // 兩條對穿的 Slide 會在盤面中心碰頭。玩家不會就這樣穿過去讓手臂交叉，
    // 而是在碰頭的瞬間互換目的地。
    let r = analyze("(120){4}1-5[4:1]/5-1[4:1],E");
    assert_eq!(r.status, "ok");
    let s = &r.solutions[0];
    let swaps: Vec<_> = s.handovers.iter().filter(|h| h.swap).collect();
    assert_eq!(swaps.len(), 2, "兩條 Slide 應該同時易手");
    assert_eq!(swaps[0].start_seconds, swaps[1].start_seconds);
    assert_ne!(swaps[0].note_id, swaps[1].note_id);
    assert_ne!(swaps[0].from, swaps[1].from);
    // 互換不需要移動，因此沒有重疊時間。
    for h in &swaps {
        near(h.end_seconds - h.start_seconds, 0.0);
    }
    // 互換之後兩手各自回到自己的半邊，不再交叉。
    let chart = chart("(120){4}1-5[4:1]/5-1[4:1],E");
    let end = chart.notes[0].motion_end.unwrap();
    let meet = swaps[0].start_seconds;
    near(hand_gap(s, meet), 0.0);
    let left = pose(&s.left_segments, end);
    let right = pose(&s.right_segments, end);
    assert!(left.x < right.x, "終點時左手應該在右手左邊");
    near(s.cost_breakdown.cross, 0.0);
    verify_segments(&s.left_segments);
    verify_segments(&s.right_segments);
    near(s.total_cost, s.cost_breakdown.total());
    // 兩條 Slide 都仍然被完整走完。
    for note in &chart.notes {
        let traced: Vec<_> = s
            .assignments
            .iter()
            .filter(|a| a.note_id == note.id && a.part == "slide")
            .collect();
        near(
            traced
                .iter()
                .map(|a| a.start_seconds)
                .fold(f64::MAX, f64::min),
            note.motion_start.unwrap(),
        );
        near(
            traced.iter().map(|a| a.end_seconds).fold(0.0, f64::max),
            note.motion_end.unwrap(),
        );
    }
}

#[test]
fn slides_that_only_cross_without_meeting_keep_the_arms_crossed() {
    // 空間上交叉但時間錯開、兩手沒有真的碰到：這種情況手臂本來就會交叉，不該硬換。
    let r = analyze("(120){4}1-4[4:1]/2-6[4:1],E");
    assert_eq!(r.status, "ok");
    let s = &r.solutions[0];
    assert!(s.handovers.iter().all(|h| !h.swap));
    let chart = chart("(120){4}1-4[4:1]/2-6[4:1],E");
    let (start, end) = (
        chart.notes[0].motion_start.unwrap(),
        chart.notes[0].motion_end.unwrap(),
    );
    let closest = (0..=40)
        .map(|k| hand_gap(s, start + (end - start) * k as f64 / 40.0))
        .fold(f64::MAX, f64::min);
    assert!(closest > 0.1, "這組本來就不會碰頭，實際最近距離 {closest}");
}

#[test]
fn swapping_follows_the_handover_switch() {
    let config = SolverConfig {
        allow_handover: false,
        ..SolverConfig::default()
    };
    let r = analyze_chart(AnalyzeRequest {
        request_id: "no-swap".into(),
        source: "(120){4}1-5[4:1]/5-1[4:1],E".into(),
        first_seconds: 0.0,
        solver_config: config,
    });
    assert_eq!(r.status, "ok");
    assert!(r.solutions.iter().all(|s| s.handovers.is_empty()));
}

#[test]
fn a_comfortable_gap_lifts_the_hand_instead_of_gliding() {
    // 相鄰鍵位但間隔拉開到連打判定間隔以上：有餘裕抬手，一般人也會抬手。
    let config = SolverConfig::default();
    let slow = analyze("(120){4}8,7,6,5,4,3,E");
    assert_eq!(slow.status, "ok");
    let chart = chart("(120){4}8,7,6,5,4,3,E");
    let step = chart.notes[1].time_seconds - chart.notes[0].time_seconds;
    assert!(step > config.repetition_seconds);
    for s in &slow.solutions {
        assert!(
            s.left_segments
                .iter()
                .chain(&s.right_segments)
                .all(|g| g.mode != "glide"),
            "間隔 {step} 秒應該抬手，不該是滑移"
        );
    }
    // 同樣的鍵位順序，接得夠緊才滑移。
    let fast = analyze("(240){24}8,7,6,5,4,3,E");
    assert!(fast.solutions[0]
        .left_segments
        .iter()
        .chain(&fast.solutions[0].right_segments)
        .any(|g| g.mode == "glide"));
}
