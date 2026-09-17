use mai_motion_core::scoring_v3::*;
use mai_motion_core::*;
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-7 * (1. + a.abs()), "{a} != {b}");
}
fn engine() -> ScoringV3 {
    ScoringV3::new(PreferenceConfigV3::default()).unwrap()
}
fn run(source: &str, c: SolverConfig) -> AnalyzeResponse {
    analyze_chart(AnalyzeRequest {
        request_id: "v3-test".into(),
        source: source.into(),
        first_seconds: 0.,
        solver_config: c,
    })
}
fn ok(source: &str) -> AnalyzeResponse {
    let r = run(source, SolverConfig::v3());
    assert_eq!(r.status, "ok", "{source}: {:?}", r.diagnostics);
    r
}
fn owners(r: &AnalyzeResponse) -> Vec<Hand> {
    r.chart
        .as_ref()
        .unwrap()
        .notes
        .iter()
        .map(|n| {
            r.solutions[0]
                .assignments
                .iter()
                .find(|a| a.note_id == n.id && a.part != "slide")
                .unwrap()
                .hand
        })
        .collect()
}
fn segment(mode: &str, a: Point, b: Point, dt: f64) -> MotionSegment {
    MotionSegment {
        mode: mode.into(),
        note_id: None,
        start_seconds: 0.,
        end_seconds: dt,
        samples: vec![MotionSample::new(0., a), MotionSample::new(dt, b)],
    }
}
fn action(time: f64, p: Point, kind: ActionKind) -> ActionEvent {
    ActionEvent {
        time_seconds: time,
        point: p,
        kind,
        hand: Hand::R,
        note_id: None,
    }
}
#[test]
fn same_point_slow_pair_stays_on_one_hand() {
    let r = ok("(120){4}1,1,E");
    let h = owners(&r);
    assert_eq!(h[0], h[1]);
}
#[test]
fn same_point_dense_four_uses_both_hands() {
    let r = ok("(240){24}1,1,1,1,E");
    let h = owners(&r);
    assert!(h.iter().any(|x| *x != h[0]), "{h:?}");
}
#[test]
fn ordinary_two_points_can_use_one_hand() {
    let r = ok("(120){4}1,2,E");
    let h = owners(&r);
    assert_eq!(h[0], h[1]);
}
#[test]
fn alternating_points_have_stable_distinct_owners() {
    let r = ok("(240){24}1,2,1,2,1,2,E");
    let h = owners(&r);
    assert_ne!(h[0], h[1], "{h:?}");
    for i in 2..6 {
        assert_eq!(h[i], h[i % 2], "{h:?}");
    }
}
#[test]
fn reversal_is_rotationally_invariant_and_not_a_jack() {
    let a = Point { x: 0.2, y: 0.3 };
    let b = Point { x: 0.8, y: -0.4 };
    let cost = reversal(a, b, a, 0.05).unwrap();
    assert!(cost > 0.);
    for deg in [90_f64, 137.] {
        let t = deg.to_radians();
        let rotate = |p: Point| Point {
            x: p.x * t.cos() - p.y * t.sin(),
            y: p.x * t.sin() + p.y * t.cos(),
        };
        near(
            cost,
            reversal(rotate(a), rotate(b), rotate(a), 0.05).unwrap(),
        );
    }
    near(reversal(a, b, Point { x: 1.4, y: -1.1 }, 0.05).unwrap(), 0.);
    near(reversal(a, a, a, 0.05).unwrap(), 0.);
    near(reversal(a, b, a, 2.).unwrap(), 0.);
}
#[test]
fn workload_recovers_and_never_charges_idle_hand() {
    let e = engine();
    let p = Point::default();
    let mut h = HandHistory::default();
    let mut costs = vec![];
    for i in 0..4 {
        costs.push(
            e.action(
                &mut h,
                &action(i as f64 * 0.04, p, ActionKind::Strike),
                0.15,
            )
            .unwrap()
            .workload,
        );
    }
    near(costs[0], 0.);
    near(costs[1], 0.);
    assert!(costs[3] > costs[1]);
    near(
        e.action(&mut h, &action(2.12, p, ActionKind::Strike), 0.15)
            .unwrap()
            .workload,
        0.,
    );
    near(
        e.action(
            &mut HandHistory::default(),
            &action(100., p, ActionKind::Strike),
            0.15,
        )
        .unwrap()
        .workload,
        0.,
    );
    near(e.score(&ScoreBreakdownV3::default()).unwrap().total(), 0.);
}
#[test]
fn jack_threshold_tolerance_and_continuous_reset() {
    let e = engine();
    let p = Point::default();
    let mut h = HandHistory::default();
    let mut j = vec![];
    for i in 0..4 {
        j.push(
            e.action(
                &mut h,
                &action(i as f64 * 0.04, p, ActionKind::Strike),
                0.15,
            )
            .unwrap()
            .jack_fatigue,
        );
    }
    near(j[0], 0.);
    near(j[1], 0.);
    assert!(j[2] > 0. && j[3] > j[2]);
    near(
        e.action(&mut h, &action(0.5, p, ActionKind::Strike), 0.15)
            .unwrap()
            .jack_fatigue,
        0.,
    );
    for kind in [ActionKind::ContinuousContact, ActionKind::Palm] {
        near(
            e.action(&mut h, &action(0.6, p, kind), 0.15)
                .unwrap()
                .jack_fatigue,
            0.,
        );
    }
    let e = ScoringV3::new(PreferenceConfigV3 {
        jack_tolerance: 100.,
        ..Default::default()
    })
    .unwrap();
    let mut h = HandHistory::default();
    for i in 0..8 {
        near(
            e.action(
                &mut h,
                &action(i as f64 * 0.04, p, ActionKind::Strike),
                0.15,
            )
            .unwrap()
            .jack_fatigue,
            0.,
        );
    }
}
#[test]
fn travel_is_not_free_and_home_control_really_reaches_zero() {
    let a = Point { x: -0.5, y: 0. };
    let b = Point { x: 0.5, y: 0. };
    let s = segment("travel", a, b, 1.);
    let t = engine().motion_terms(&s, Hand::L).unwrap();
    assert!(t.travel > 0.);
    near(t.speed_strain, 0.);
    let mut previous = 0.;
    for home in [0., 50., 100.] {
        let e = ScoringV3::new(PreferenceConfigV3 {
            home_preference: home,
            ..Default::default()
        })
        .unwrap();
        let t = e.motion_terms(&s, Hand::L).unwrap();
        if home == 0. {
            near(t.excursion, 0.);
        } else {
            assert!(t.excursion > previous);
        }
        previous = t.excursion;
    }
    near(
        engine()
            .motion_terms(&segment("tap", b, b, 0.03), Hand::L)
            .unwrap()
            .excursion,
        0.,
    );
    near(
        engine()
            .motion_terms(&segment("slide", a, b, 1.), Hand::L)
            .unwrap()
            .travel,
        0.,
    );
}
#[test]
fn speed_is_soft_and_comfort_monotone() {
    for speed in [30., 60., 90., 120., 180.] {
        let mut old = f64::INFINITY;
        for comfort in [1., 30., 90., 200.] {
            let e = ScoringV3::new(PreferenceConfigV3 {
                travel_comfort: comfort,
                ..Default::default()
            })
            .unwrap();
            let cost = e.movement_strain(speed, 1.).unwrap();
            assert!(cost.is_finite() && cost >= 0. && cost <= old);
            old = cost;
        }
    }
    assert!(engine().movement_strain(1., 0.).is_err());
    near(engine().movement_strain(0., 0.).unwrap(), 0.);
}
#[test]
fn busy_hand_can_accept_help_across_midline() {
    let e = engine();
    let mut right = HandHistory::default();
    let a = Point { x: 0.2, y: 0. };
    let b = Point { x: 0.9, y: 0. };
    for (i, p) in [a, b, a, b].into_iter().enumerate() {
        e.action(
            &mut right,
            &action(i as f64 * 0.04, p, ActionKind::Strike),
            0.15,
        )
        .unwrap();
    }
    let rp = e
        .action(&mut right, &action(0.16, a, ActionKind::Strike), 0.15)
        .unwrap();
    let lp = e
        .action(
            &mut HandHistory::default(),
            &action(0.16, a, ActionKind::Strike),
            0.15,
        )
        .unwrap();
    let rm = e
        .motion_terms(&segment("travel", b, a, 0.01), Hand::R)
        .unwrap();
    let lm = e
        .motion_terms(
            &segment("travel", Point { x: -0.2, y: 0. }, a, 0.16),
            Hand::L,
        )
        .unwrap();
    assert!(
        lm.travel + lm.excursion + lp.workload
            < rm.travel + rm.excursion + rp.reversal + rp.workload
    );
}
#[test]
fn wire_defaults_validation_and_round_trip() {
    let c: SolverConfig = serde_json::from_str(r#"{"scoringModel":"human-motion-v3"}"#).unwrap();
    near(c.home_preference, 60.);
    near(c.travel_comfort, 90.);
    near(c.jack_tolerance, 60.);
    for key in [
        "repeatTolerance",
        "distanceWeight",
        "speedWeight",
        "sideWeight",
        "crossWeight",
        "repetitionWeight",
        "handoverWeight",
        "speedReference",
    ] {
        assert!(serde_json::from_value::<SolverConfig>(
            serde_json::json!({"scoringModel":"human-motion-v3",key:1})
        )
        .is_err());
    }
    for model in ["legacy-v1", "hand-affinity-v2"] {
        assert!(serde_json::from_value::<SolverConfig>(
            serde_json::json!({"scoringModel":model,"jackTolerance":60})
        )
        .is_err());
    }
    let v = serde_json::to_value(c).unwrap();
    assert!(v.get("jackTolerance").is_some());
    assert!(v.get("repeatTolerance").is_none());
    assert!(v.get("distanceWeight").is_none());
    for value in [-1., 101., f64::NAN, f64::INFINITY] {
        assert!(SolverConfig {
            jack_tolerance: value,
            ..SolverConfig::v3()
        }
        .validate()
        .is_err());
    }
    let r = ok("(120){4}1,2,E");
    assert_eq!(r.schema_version, 4);
    let v = serde_json::to_value(&r).unwrap();
    assert!(v["solutions"][0].get("totalCost").is_none());
    let decoded: AnalyzeResponse = serde_json::from_value(v).unwrap();
    assert!(decoded.solutions[0]
        .score
        .as_ref()
        .unwrap()
        .as_v3()
        .is_some());
}
fn continuity(s: &[MotionSegment]) {
    for pair in s.windows(2) {
        near(pair[0].end_seconds, pair[1].start_seconds);
        near(
            pair[0]
                .samples
                .last()
                .unwrap()
                .point()
                .distance(pair[1].samples[0].point()),
            0.,
        );
    }
    for seg in s {
        for p in seg.samples.windows(2) {
            assert!(p[1].time_seconds >= p[0].time_seconds);
            if p[1].time_seconds == p[0].time_seconds {
                near(p[0].point().distance(p[1].point()), 0.);
            }
        }
    }
}
#[test]
fn corpus_feasibility_continuity_and_full_incremental_verification() {
    let cases = [
        "(120){4}1,2,3,4,E",
        "(120){4}1,8,2,7,E",
        "(120){4}1/8,2/7,E",
        "(120){4}1h[4:2],7,6,E",
        "(120){4}C/B1/E1/7,E",
        "(120){8}Ch[4:2]/1,B1/2,B2/3,E",
        "(240){24}8,7,6,5,4,3,E",
        "(240){24}1,1,1,1,E",
        "(120){4}1-5[4:3],8,7,6,E",
        "(120){4}1-5[4:1],3/7,E",
        "(145){8}1-5[8:1]/8-4[8:1],27,,18,E",
        "(120){4}1-5[4:1]/5-1[4:1],E",
        "(120){4}1-4[4:1]/2-6[4:1],E",
        "(120){4}1?-5[4:1],E",
        "(120){4}1-3[8:1]-5[8:1],E",
        "(120){4}1p5[4:1],E",
        "(120){4}1q5[4:1],E",
        "(120){4}1w5[4:1],E",
        "(120){4}1s5[4:1],E",
        "(240){8}4^1[4:1]/8^5[4:1],,4b/8b,,,,,,E",
    ];
    for source in cases {
        let r = ok(source);
        assert_eq!(run(source, SolverConfig::v2()).status, r.status);
        for s in &r.solutions {
            continuity(&s.left_segments);
            continuity(&s.right_segments);
            let p = s.score_breakdown.as_ref().unwrap().as_v3().unwrap();
            let score = s.score.as_ref().unwrap().as_v3().unwrap();
            near(score.total(), p.values().iter().sum());
            near(
                score.total(),
                score.movement() + score.posture() + score.fatigue(),
            );
        }
    }
}
#[test]
fn determinism_and_simultaneous_order() {
    let a = ok("(120){4}C/B1/E1/7,1/8,E");
    let b = ok("(120){4}7/E1/B1/C,8/1,E");
    near(
        a.solutions[0]
            .score
            .as_ref()
            .unwrap()
            .as_v3()
            .unwrap()
            .total(),
        b.solutions[0]
            .score
            .as_ref()
            .unwrap()
            .as_v3()
            .unwrap()
            .total(),
    );
    assert_eq!(
        serde_json::to_value(&a).unwrap(),
        serde_json::to_value(ok("(120){4}C/B1/E1/7,1/8,E")).unwrap()
    );
}

#[test]
fn checkpoints_do_not_multiply_cost_and_late_pickup_keeps_compression() {
    let source = "(120){4}1?-5[4:1],E";
    let a = run(
        source,
        SolverConfig {
            allow_handover: false,
            ..SolverConfig::v3()
        },
    );
    let b = run(
        source,
        SolverConfig {
            allow_handover: false,
            checkpoint_seconds: 0.025,
            handover_seconds: 0.02,
            ..SolverConfig::v3()
        },
    );
    assert_eq!(a.status, "ok");
    assert_eq!(b.status, "ok");
    let pa = a.solutions[0]
        .score_breakdown
        .as_ref()
        .unwrap()
        .as_v3()
        .unwrap();
    let pb = b.solutions[0]
        .score_breakdown
        .as_ref()
        .unwrap()
        .as_v3()
        .unwrap();
    for (x, y) in pa.values().into_iter().zip(pb.values()) {
        near(x, y);
    }
    near(pa.jack_fatigue + pa.reversal + pa.workload, 0.);
    let late = run(
        "(120){4}1-5[4:1],3/7,E",
        SolverConfig {
            travel_comfort: 1.,
            ..SolverConfig::v3()
        },
    );
    assert_eq!(late.status, "ok");
    assert!(
        late.solutions[0]
            .score_breakdown
            .as_ref()
            .unwrap()
            .as_v3()
            .unwrap()
            .compression_strain
            > 0.
    );
    let zero = run(
        "(120){4}1-5[4:1]/5-1[4:1],E",
        SolverConfig {
            home_preference: 0.,
            ..SolverConfig::v3()
        },
    );
    assert_eq!(zero.status, "ok");
    for s in zero.solutions {
        near(
            s.score_breakdown
                .as_ref()
                .unwrap()
                .as_v3()
                .unwrap()
                .excursion,
            0.,
        );
    }
}

#[test]
fn handover_has_a_floor_and_v3_keeps_crossing_slide_owners() {
    let willing = ScoringV3::new(PreferenceConfigV3 {
        handover_willingness: 100.,
        ..Default::default()
    })
    .unwrap();
    near(willing.handover(false), 0.1);
    near(willing.handover(true), 0.);
    let r = ok("(120){4}1-5[4:1]/5-1[4:1],E");
    assert_eq!(
        r.solutions[0].handovers.iter().filter(|h| h.swap).count(),
        0
    );
    near(
        r.solutions[0]
            .score_breakdown
            .as_ref()
            .unwrap()
            .as_v3()
            .unwrap()
            .handover,
        0.,
    );
}
