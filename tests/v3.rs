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
fn dense_repetition_saturation_grows_without_forcing_a_hand() {
    // Only the cost is asserted; whether Top-1 changes hand depends on context.
    let e = engine();
    let p = Point::default();
    let run = |gap: f64, count: usize| {
        let mut h = HandHistory::default();
        (0..count)
            .map(|i| {
                e.action(&mut h, &action(i as f64 * gap, p, ActionKind::Strike), 0.15)
                    .unwrap()
                    .jack_fatigue
            })
            .collect::<Vec<_>>()
    };
    let dense = run(0.03, 7);
    for j in &dense[..3] {
        near(*j, 0.);
    }
    for pair in dense[3..].windows(2) {
        assert!(pair[0] > 0. && pair[1] > pair[0], "{dense:?}");
    }
    let slower = run(0.09, 7);
    assert!(slower[6] < dense[6], "{slower:?} {dense:?}");
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
    for i in 0..5 {
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
    near(j[2], 0.);
    assert!(j[3] > 0. && j[4] > j[3]);
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
    near(c.travel_comfort, 6.5);
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

// ---- V3.1 local phrase planning / ownership ----

fn owners_in(r: &AnalyzeResponse, from: usize, len: usize) -> (Vec<u8>, Vec<Hand>) {
    let notes = &r.chart.as_ref().unwrap().notes[from..from + len];
    let hands = owners(r)[from..from + len].to_vec();
    (notes.iter().map(|n| n.button).collect(), hands)
}
/// Same hand inside every lane (relations only; L/R mirroring is allowed),
/// different hands for each `apart` pair.
fn assert_lanes(h: &[Hand], same: &[&[usize]], apart: &[(usize, usize)], label: &str) {
    for lane in same {
        for i in lane.iter() {
            assert_eq!(h[*i], h[lane[0]], "{label}: lane {lane:?} in {h:?}");
        }
    }
    for (a, b) in apart {
        assert_ne!(h[*a], h[*b], "{label}: {a} vs {b} in {h:?}");
    }
}
fn button_point(k: u8) -> Point {
    let t = -std::f64::consts::FRAC_PI_2
        + std::f64::consts::FRAC_PI_8
        + (k as f64 - 1.) * std::f64::consts::FRAC_PI_4;
    Point {
        x: t.cos(),
        y: t.sin(),
    }
}

#[test]
fn ownership_strengthens_switch_costs_and_decays() {
    let key = ContactKey::Button(6);
    let mut o = OwnershipState::default();
    near(o.switch_cost(key, Hand::R, 0.), 0.);
    o.observe(key, Hand::L, 0.);
    let first = o.get(key, 0.).unwrap().1;
    o.observe(key, Hand::L, 0.125);
    let (owner, second) = o.get(key, 0.125).unwrap();
    assert_eq!(owner, Hand::L);
    assert!(second > first, "{second} <= {first}");
    near(o.switch_cost(key, Hand::L, 0.25), 0.);
    let soon = o.switch_cost(key, Hand::R, 0.25);
    assert!(soon > 0.);
    let later = o.switch_cost(key, Hand::R, 1.);
    assert!(later > 0. && later < soon);
    near(o.switch_cost(key, Hand::R, 5.), 0.);
    o.decay_to(5.);
    assert_eq!(o.entries().count(), 0);
    // A switch hands the key over with only the initial strength.
    o.observe(key, Hand::L, 6.);
    o.observe(key, Hand::L, 6.1);
    o.observe(key, Hand::R, 6.2);
    assert_eq!(o.get(key, 6.2).unwrap().0, Hand::R);
    near(o.get(key, 6.2).unwrap().1, OWNERSHIP_INITIAL);
}

#[test]
fn an_anchored_hand_pays_for_notes_the_free_hand_could_take() {
    // 6 6 8 6: L is about to play 6 again, so the interleaved 8 belongs to the
    // free hand; L taking it pays for leaving its anchor.
    let mut single = RoleState::default();
    single.observe(
        Hand::L,
        button_point(6),
        ContactKey::Button(6),
        Some(0.25),
        0.,
    );
    let eight = ContactKey::Button(8);
    near(
        single.contact_cost(Hand::R, button_point(8), eight, 0.125, false),
        0.,
    );
    assert!(single.contact_cost(Hand::L, button_point(8), eight, 0.125, false) > 0.);
    // Playing the anchor itself is always free.
    near(
        single.contact_cost(
            Hand::L,
            button_point(6),
            ContactKey::Button(6),
            0.125,
            false,
        ),
        0.,
    );
    // The role expires at the anchor visit; later notes are free again.
    near(
        single.contact_cost(Hand::L, button_point(8), eight, 0.5, false),
        0.,
    );

    // 1/3 chord: 1 is revisited later (outer anchor), 3 sooner (inner lane).
    // With both hands anchored only the outer hand gives way.
    let mut roles = RoleState::default();
    roles.observe(
        Hand::L,
        button_point(1),
        ContactKey::Button(1),
        Some(0.75),
        0.,
    );
    roles.observe(
        Hand::R,
        button_point(3),
        ContactKey::Button(3),
        Some(0.5),
        0.,
    );
    let two = ContactKey::Button(2);
    near(
        roles.contact_cost(Hand::R, button_point(2), two, 0.25, false),
        0.,
    );
    assert!(roles.contact_cost(Hand::L, button_point(2), two, 0.25, false) > 0.);
    // A shorter nested repeat keeps the outer anchor of that hand.
    let mut nested = roles.clone();
    nested.observe(Hand::R, button_point(2), two, Some(0.375), 0.25);
    assert_eq!(nested.roles[1].anchor, Some(ContactKey::Button(3)));
    // Returning to the anchor with no further visit ends the role.
    nested.observe(Hand::R, button_point(3), ContactKey::Button(3), None, 0.5);
    assert!(nested.roles[1].anchor.is_none());
}

#[test]
fn anchored_reversal_is_smaller_than_plain_reversal() {
    let e = engine();
    let (a, b) = (button_point(3), button_point(2));
    let strike = |h: &mut HandHistory, t: f64, p: Point, scale: f64| {
        e.action_with(h, &action(t, p, ActionKind::Strike), 0.15, scale)
            .unwrap()
            .reversal
    };
    let mut history = HandHistory::default();
    strike(&mut history, 0., a, 1.);
    strike(&mut history, 0.125, b, 1.);
    let plain = strike(&mut history.clone(), 0.25, a, 1.);
    assert!(plain > 0.);

    let mut roles = RoleState::default();
    roles.observe(Hand::R, a, ContactKey::Button(3), Some(0.25), 0.);
    let scale = roles.reversal_scale(&history, Hand::R, a, 0.25, false);
    near(scale, ANCHOR_REVERSAL_DISCOUNT);
    let anchored = strike(&mut history.clone(), 0.25, a, scale);
    assert!(anchored > 0. && anchored < plain, "{anchored} / {plain}");
    // ABAB: the excursion target is needed again soon, so no discount.
    near(roles.reversal_scale(&history, Hand::R, a, 0.25, true), 1.);
    // No stable role: plain reversal.
    near(
        RoleState::default().reversal_scale(&history, Hand::R, a, 0.25, false),
        1.,
    );
}

#[test]
fn regression_6686_keeps_the_repeated_key_owner() {
    for source in [
        "(240){8}6,6,8,6,E",
        "(150){16}6,6,8,6,E",
        "(120){16}6,6,8,6,E",
    ] {
        let h = owners(&ok(source));
        assert_lanes(&h, &[&[0, 1, 3]], &[(0, 2)], source);
    }
}

#[test]
fn regression_66856_keeps_the_repeated_key_owner() {
    for source in ["(240){8}6,6,8,5,6,E", "(120){16}6,6,8,5,6,E"] {
        let h = owners(&ok(source));
        // 8 may use the other hand; 5 is left to geometry and context.
        assert_lanes(&h, &[&[0, 1, 4]], &[], source);
    }
}

/// `13 2 2 3 1 68 7 7 6 8` in parse order.
const LOCAL_ROLE_BUTTONS: [u8; 12] = [1, 3, 2, 2, 3, 1, 6, 8, 7, 7, 6, 8];
/// Priority: burst speed > left/right regions > keeping the same owner.
/// Chords split; the hand that just played the 16th repeat does not also
/// rush to the next key 0.125 s later when the other hand can share it.
const LOCAL_ROLE_APART: [(usize, usize); 4] = [(0, 1), (6, 7), (3, 4), (9, 10)];
/// Sanity bound only: the player reference for n780 moves a hand 1.414 radii
/// in 0.19 s (7.5 radii/s), so a phrase may legitimately reach that.
const PHRASE_MAX_HAND_SPEED: f64 = 8.0;

/// Fastest contact-to-contact move of one hand inside the phrase (radii/s).
fn phrase_peak_speed(r: &AnalyzeResponse, from: usize, len: usize) -> f64 {
    let notes = &r.chart.as_ref().unwrap().notes[from..from + len];
    let hands = &owners(r)[from..from + len];
    let mut last: [Option<&Note>; 2] = [None, None];
    let mut peak: f64 = 0.;
    for (note, hand) in notes.iter().zip(hands) {
        if let Some(prev) = last[hand.index()] {
            let dt = note.time_seconds - prev.time_seconds;
            if dt > 1e-9 {
                peak = peak.max(prev.position.distance(note.position) / dt);
            }
        }
        last[hand.index()] = Some(note);
    }
    peak
}

fn assert_shared_phrase(r: &AnalyzeResponse, from: usize, len: usize, label: &str) {
    let (buttons, h) = owners_in(r, from, len);
    assert_eq!(buttons, LOCAL_ROLE_BUTTONS[..len]);
    let apart: Vec<_> = LOCAL_ROLE_APART
        .iter()
        .copied()
        .filter(|(a, b)| *a < len && *b < len)
        .collect();
    assert_lanes(&h, &[], &apart, label);
    let peak = phrase_peak_speed(r, from, len);
    assert!(
        peak <= PHRASE_MAX_HAND_SPEED,
        "{label}: peak {peak:.2} in {h:?}"
    );
}

#[test]
fn burst_speed_charges_only_genuinely_fast_moves() {
    let e = engine();
    let (a, b) = (button_point(1), button_point(3));
    let burst = |from: Point, to: Point, dt: f64| {
        e.motion_terms(&segment("travel", from, to, dt), Hand::R)
            .unwrap()
            .speed_strain
    };
    // Player reference (n780): 1 -> 3 in 0.19 s (7.4 radii/s) is normal play.
    let reference = burst(a, b, 0.19);
    assert!(reference > 0. && reference < 0.1, "{reference}");
    // The same move at 137 BPM 8th spacing is free.
    near(burst(a, b, 0.25), 0.);
    // Twice as fast is not twice as expensive.
    let rushed = burst(a, b, 0.095);
    assert!(rushed > 6. * reference, "{rushed} / {reference}");
    // Same speed, twice the distance and twice the time: twice the cost.
    let far = Point {
        x: 2. * b.x - a.x,
        y: 2. * b.y - a.y,
    };
    near(burst(a, far, 0.38), 2. * reference);
    near(burst(a, b, 1.), 0.);
}

/// The n780 reference move: 1 -> 3 in 0.19 s.
fn burst_reference() -> f64 {
    engine()
        .motion_terms(
            &segment("travel", button_point(1), button_point(3), 0.19),
            Hand::R,
        )
        .unwrap()
        .speed_strain
}

#[test]
fn one_hand_density_is_what_makes_sharing_worth_it() {
    // Four strikes on one hand: at 16th spacing the hand saturates, at 8th
    // spacing it stays near the free level. This is why a dense run is shared
    // while a slower repeat stays on the same hand.
    let e = engine();
    let p = button_point(1);
    let run = |gap: f64| {
        let mut h = HandHistory::default();
        (0..4)
            .map(|i| {
                e.action(&mut h, &action(i as f64 * gap, p, ActionKind::Strike), 0.15)
                    .unwrap()
                    .workload
            })
            .sum::<f64>()
    };
    let dense = run(0.109);
    assert!(dense > 1., "{dense}");
    // 8th spacing at 137 BPM stays under the free level entirely.
    near(run(0.219), 0.);
    near(run(1.5), 0.);
    // And it outweighs the burst of moving a key away at that spacing.
    assert!(dense > 10. * burst_reference(), "{dense}");
}

/// Full transcribed charts are player-supplied reference material and stay out
/// of the repository (see .gitignore). The phrase-level behaviour they cover is
/// also asserted on the excerpts above; this test adds the whole-chart context
/// wherever the file is available.
#[test]
fn regression_full_chart_n488_to_n499_local_roles() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/charts/local_role_full.txt"
    );
    let Ok(source) = std::fs::read_to_string(path) else {
        eprintln!("skipped: {path} is not present");
        return;
    };
    let r = ok(&source);
    // 0-based n489..n500 is the reported n488~n499 phrase; n493 must not be
    // a 0.125 s rush by the hand that just played n492.
    assert_shared_phrase(&r, 489, 12, "n489");
    // Crossing is not banned, only sustained crossing: brief swaps survive.
    let last = r
        .chart
        .as_ref()
        .unwrap()
        .notes
        .iter()
        .map(|n| n.end_seconds)
        .fold(0., f64::max);
    let episodes = swapped_episodes(&r.solutions[0], last);
    assert!(!episodes.is_empty(), "crossing must not be forbidden");
    assert!(episodes.iter().all(|e| *e < 1.), "{episodes:?}");
}

#[test]
fn temporary_helper_returns_after_the_local_phrase() {
    // The other hand helps inside a left-side cluster; the right-side demand
    // right after it must be taken by a hand that left the cluster.
    for source in [
        "(150){16}6,7,6,7,6,7,{8}2,3,E",
        "(150){8}2,{16}6,7,6,7,6,7,{8}3,2,E",
    ] {
        let r = ok(source);
        let h = owners(&r);
        let n = h.len();
        let phrase: std::collections::BTreeSet<_> =
            h[n - 8..n - 2].iter().map(|x| x.index()).collect();
        assert_eq!(
            phrase.len(),
            2,
            "{source}: both hands share the phrase {h:?}"
        );
        assert_eq!(h[n - 2], h[n - 1], "{source}: {h:?}");
        assert_eq!(h[n - 1], Hand::R, "{source}: {h:?}");
        for s in &r.solutions {
            continuity(&s.left_segments);
            continuity(&s.right_segments);
        }
    }
}

#[test]
fn ownership_switch_is_reported_and_debug_trace_shows_roles() {
    let r = ok("(240){8}6,6,8,6,E");
    let p = r.solutions[0]
        .score_breakdown
        .as_ref()
        .unwrap()
        .as_v3()
        .unwrap();
    let score = r.solutions[0].score.as_ref().unwrap().as_v3().unwrap();
    near(score.total(), p.values().iter().sum());
    near(
        score.posture(),
        p.excursion + p.cross_exposure + p.handover + p.ownership_switch,
    );
    let v = serde_json::to_value(p).unwrap();
    assert!(v.get("ownershipSwitch").is_some());
    let chart = parse_chart("(120){8}13,{16}2,2,{8}3,1,E", 0.)
        .unwrap()
        .chart;
    let lines = trace_v3(&chart, &SolverConfig::v3()).unwrap();
    assert_eq!(lines.len(), 6, "{lines:#?}");
    assert!(
        lines[1].contains("anchor 1") && lines[1].contains("anchor 3"),
        "{lines:#?}"
    );
    assert!(lines
        .iter()
        .all(|l| l.contains("future:") && l.contains("ownership:")));
}

#[test]
fn short_hold_is_left_early_instead_of_forcing_one_hand_and_crossing() {
    // 137 BPM: 4/5 short holds, then 1,8,1,8 16ths, then 4/5 and 2/7 chords
    // (0-based n715..n726 of tests/charts/speed_first_full.txt).
    let r =
        ok("(137){16}2hx[16:3]/7hx[16:3],,,,4x/5x,4hx[8:1]/5hx[8:1],,,1x,8,1,8,,4x/5x,,2b/7b,E");
    let chart = r.chart.as_ref().unwrap();
    let h = owners(&r);
    let buttons: Vec<u8> = chart.notes.iter().map(|n| n.button).collect();
    assert_eq!(buttons, [2, 7, 4, 5, 4, 5, 1, 8, 1, 8, 4, 5, 2, 7]);
    // The 16th top alternation is shared by both hands.
    for i in 6..9 {
        assert_ne!(h[i], h[i + 1], "{h:?}");
    }
    // Chords keep regions: the left key goes to L.
    for (right, left) in [(10, 11), (12, 13)] {
        assert_eq!(h[left], Hand::L, "{h:?}");
        assert_eq!(h[right], Hand::R, "{h:?}");
    }
    // Both short holds end at their earliest legal release before moving.
    let s = &r.solutions[0];
    for note in &chart.notes[4..6] {
        let segments = if s
            .assignments
            .iter()
            .any(|a| a.note_id == note.id && a.hand == Hand::L)
        {
            &s.left_segments
        } else {
            &s.right_segments
        };
        let hold = segments
            .iter()
            .find(|g| g.mode == "hold" && g.note_id.as_deref() == Some(note.id.as_str()))
            .unwrap();
        assert!(
            hold.end_seconds < note.end_seconds - 1e-6,
            "{} held to the end",
            note.id
        );
    }
    for s in &r.solutions {
        continuity(&s.left_segments);
        continuity(&s.right_segments);
    }
}

/// Swapped posture episodes from real trajectories (L x > 0.2 while R x < -0.2),
/// sampled every 10 ms up to `until`.
fn swapped_episodes(s: &Solution, until: f64) -> Vec<f64> {
    let at = |segments: &[MotionSegment], t: f64| {
        let g = segments
            .iter()
            .find(|g| g.start_seconds <= t && t <= g.end_seconds)?;
        let k = g
            .samples
            .partition_point(|p| p.time_seconds < t)
            .clamp(1, g.samples.len() - 1);
        let (a, b) = (&g.samples[k - 1], &g.samples[k]);
        let span = b.time_seconds - a.time_seconds;
        let u = if span > 0. {
            ((t - a.time_seconds) / span).clamp(0., 1.)
        } else {
            1.
        };
        Some(a.point().lerp(b.point(), u))
    };
    let (mut t, mut run, mut runs) = (s.left_segments[0].start_seconds, 0., vec![]);
    while t < until {
        match (at(&s.left_segments, t), at(&s.right_segments, t)) {
            (Some(l), Some(r)) if l.x > 0.2 && r.x < -0.2 => run += 0.01,
            _ if run > 0. => {
                runs.push(run);
                run = 0.;
            }
            _ => {}
        }
        t += 0.01;
    }
    if run > 0. {
        runs.push(run);
    }
    runs
}

#[test]
fn swapped_posture_accumulates_but_same_side_and_brief_swaps_stay_cheap() {
    let e = engine();
    let (right, left) = (button_point(4), button_point(5));
    let hold = |p: Point, dt: f64| segment("hold", p, p, dt);
    // Both hands on their own side, or both on one side: no swapped posture.
    near(e.crossing(&hold(left, 1.), &hold(right, 1.)).unwrap(), 0.);
    near(
        e.crossing(&segment("idle", left, left, 1.), &hold(button_point(7), 1.))
            .unwrap(),
        0.,
    );
    // Swapped near the midline (adjacent bottom keys) is still charged, and
    // cost grows with how long the swap is held; waiting counts too.
    let brief = e
        .crossing(
            &segment("idle", right, right, 0.1),
            &segment("idle", left, left, 0.1),
        )
        .unwrap();
    let long = e
        .crossing(
            &segment("idle", right, right, 1.),
            &segment("idle", left, left, 1.),
        )
        .unwrap();
    assert!(brief > 0. && long > 5. * brief, "{brief} / {long}");
    // Nothing is charged after the chart's last demand.
    near(
        e.crossing_until(
            &segment("idle", right, right, 1.),
            &segment("idle", left, left, 1.),
            0.,
        )
        .unwrap(),
        0.,
    );
    near(e.swapped_rate(left, right), 0.);
    assert!(e.swapped_rate(right, left) > 0.);
}

#[test]
fn repeated_midline_chords_do_not_stay_swapped() {
    // Last five lines of tests/charts/speed_first_full.txt: after the 17 and
    // 2/6 chords the hands must not keep 4 (L) / 5 (R) through 45,45,45,4h/5h.
    let source = "(137){16}8,,8,,6,5,,45,,3,4,,25,,1,8,1,,1,,3,4,,45,,6,5,,47,,8,,2,1,2b,,7,8,7b,,3,4,3b,,6,5,6b,,2,2h[8:1],,4,4h[8:1],,6,6h[8:1],,8,,17,,2b/6b,,4,5,,45,,45,,45,4h[4:1]/5h[4:1],,,,,B2/B3/B6/B7/Cf,{1},,,,,,,,,E";
    let r = ok(source);
    let chart = r.chart.as_ref().unwrap();
    let last = chart
        .notes
        .iter()
        .map(|n| n.end_seconds.max(n.motion_end.unwrap_or(n.time_seconds)))
        .fold(0., f64::max);
    let episodes = swapped_episodes(&r.solutions[0], last);
    assert!(
        episodes.iter().all(|e| *e < 1.),
        "{episodes:?} {:?}",
        owners(&r)
    );
}

#[test]
fn a_hand_about_to_replay_a_key_is_not_pulled_away() {
    // speed_first_full.txt around n780: 25 chord, then 1,8,1,,1. The hand that
    // just played 1 and plays it again 0.219 s later must not be the one that
    // takes the 8 in between; the other hand is free to reach it.
    let source = "(137){8}1-5[8:1]/8,B2/B3/E3,1/8-4[8:1],B6/B7/E7,18,B2/B3/E3,1b/6b,,{16}2hx[16:3]/7hx[16:3],,,,4x/5x,4hx[8:1]/5hx[8:1],,,1x,8,1,8,,4x/5x,,2b/7b,,,1,,3,4,,45,,6,5,,47,,8,1,8,,8,,6,5,,45,,3,4,,25,,1,,{8}2h[4:1]/6,1,4h[4:1],1,5h[4:1],8,7h[4:1],8,{16}4,,6,56,,3,3/4h[4:1],,,,{24}1,8,7,2b/6b,,,,,,{16}8,,8,,6,5,,45,,3,4,,25,,1,8,1,,1,,3,4,,45,,6,5,,47,,8,,E";
    let r = ok(source);
    let chart = r.chart.as_ref().unwrap();
    let h = owners(&r);
    let at = |index: usize| (chart.notes[index].button, h[index]);
    let buttons: Vec<u8> = chart.notes.iter().map(|n| n.button).collect();
    let start = buttons
        .windows(4)
        .position(|w| w == [1, 8, 1, 1])
        .expect("1,8,1,1 group");
    // Player reference for n780..n787: R L R R R L R L (mirrored is fine).
    assert_eq!(
        (0..8).map(|i| at(start + i).0).collect::<Vec<_>>(),
        [1, 8, 1, 1, 3, 4, 4, 5]
    );
    let (a, b) = (at(start).1, at(start + 1).1);
    assert_ne!(a, b, "the 8 belongs to the other hand: {h:?}");
    for (i, hand) in [a, b, a, a, a, b, a, b].iter().enumerate() {
        assert_eq!(at(start + i).1, *hand, "n780+{i} in {h:?}");
    }
}

#[test]
fn player_reference_chord_tail_and_three_note_figures() {
    // speed_first_full.txt lines 68-70 (n788..n798): 6,5 lead-in, the 4/7
    // chord, its trailing 8, then the 2,1,2 and 7,8,7 figures.
    // Player reference: L R R L L R L R L R L (mirrored is fine).
    let source = "(137){16}4,,6,56,,3,3/4h[4:1],,,,{24}1,8,7,2b/6b,,,,,,{16}8,,8,,6,5,,45,,3,4,,25,,1,8,1,,1,,3,4,,45,,6,5,,47,,8,,2,1,2b,,7,8,7b,,3,4,3b,,6,5,6b,,E";
    let r = ok(source);
    let chart = r.chart.as_ref().unwrap();
    let h = owners(&r);
    let buttons: Vec<u8> = chart.notes.iter().map(|n| n.button).collect();
    let at = buttons
        .windows(11)
        .position(|w| w == [6, 5, 4, 7, 8, 2, 1, 2, 7, 8, 7])
        .expect("figure");
    let (x, y) = (h[at], h[at + 1]);
    assert_ne!(x, y, "6 and 5 split: {h:?}");
    for (i, hand) in [x, y, y, x, x, y, x, y, x, y, x].iter().enumerate() {
        assert_eq!(h[at + i], *hand, "note {i} of the figure in {h:?}");
    }
}
