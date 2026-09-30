//! V3 公式機制測試。預設權重是依真人標註擬合的結果，部分項目（單手負荷、移動距離、
//! 離開 anchor）目前擬合為 0；這裡固定使用擬合前的權重，確認各項公式本身的形狀仍然正確，
//! 日後重新擬合讓它們回到非零時可以直接使用。整個測試程式共用同一組權重。
use mai_motion_core::scoring_v3::*;
use mai_motion_core::*;

/// 擬合前（v3-3）的權重與快速移動容忍。
fn engine() -> ScoringV3 {
    set_tuning(TuningV3 {
        travel: 0.18,
        speed_strain: 1.0,
        swapped_posture: 3.0,
        contact_cross: 1.0,
        home_entry: 1.0,
        home_exposure: 0.10,
        reversal: 0.28,
        jack: 0.45,
        workload: 4.0,
        workload_tau: 0.45,
        workload_free: 1.25,
        same_point_load: 1.0,
        same_point_window: 0.3,
        ownership_switch: 0.45,
        anchor_hold: 0.45,
        chord_switch_discount: 0.3,
        star_switch: 0.0,
        stair_break: 0.0,
        stair_window: 0.3,
        stair_fast: 0.0,
        stair_speed: 0.0,
        stroke_break: 0.0,
        stroke_window: 0.15,
        stroke_step: 0.55,
        shape_mix: 0.0,
        phrase_template: 0.0,
        glide_speed: 1.0,
        future_role: 0.15,
        arc_side: 0.0,
        split_upper: 0.0,
        split_window: 0.5,
    });
    ScoringV3::new(PreferenceConfigV3 {
        travel_comfort: 6.5,
        ..Default::default()
    })
    .unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-7 * (1. + a.abs()), "{a} != {b}");
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
fn an_anchored_hand_pays_for_notes_the_free_hand_could_take() {
    engine();
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
