//! Transition audit: inspect physical events, not merely the reported score.
use super::*;
use crate::scoring_v3::ContactKey;
fn initial() -> State {
    State {
        v2: Default::default(),
        v3: Default::default(),
        actions: Default::default(),
        group_origin: 0,
        arms: [
            Arm {
                point: button(7),
                free: -1.,
                last_tap: None,
                segments: Default::default(),
            },
            Arm {
                point: button(2),
                free: -1.,
                last_tap: None,
                segments: Default::default(),
            },
        ],
        cost: Default::default(),
        assignments: Default::default(),
        handovers: Default::default(),
        palms: Default::default(),
        active_palms: [None, None],
        held_touch: [None, None],
        owners: Default::default(),
        wifi_pairs: Default::default(),
        engaged: Default::default(),
        pending: 0.,
        deposit: Default::default(),
        last_handover: Default::default(),
        used_touch_sweep: false,
        lenient_in_group: false,
        finished: Default::default(),
        used_early_slide: false,
        used_touch_group: false,
    }
}
#[test]
fn slide_handover_remains_available_before_another_contact() {
    let c = SolverConfig::v3();
    for (source, allowed) in [
        ("(120){4}1-5[4:2],E", false),
        ("(120){4}1-5[4:2],{8},8,E", true),
    ] {
        let chart = parse_chart(source, 0.0).unwrap().chart;
        let ts = tasks(&chart, &c).unwrap();
        let mut slide = ts.iter().filter(|t| t.mode == "slide");
        let first = slide.next().unwrap();
        let second = slide.next().unwrap();
        let old = assign(&initial(), first, Hand::R, &chart, &c, false).remove(0);
        let switched = assign(&old, second, Hand::L, &chart, &c, false);
        assert_eq!(!switched.is_empty(), allowed);
        if allowed {
            let next = &switched[0];
            let h = next.handovers.last().unwrap();
            assert_eq!(h.from, Hand::R);
            assert_eq!(h.to, Hand::L);
            assert!((h.end_seconds - h.start_seconds - c.handover_seconds).abs() < EPS);
            assert_eq!(next.arms[Hand::R.index()].free, h.end_seconds);
        }
    }
}
#[test]
fn physical_strikes_and_glide_rewrite_keep_original_action() {
    let c = SolverConfig::v3();
    let chart = parse_chart("(240){24}1,2,3h[4:1],E", 0.).unwrap().chart;
    let ts = tasks(&chart, &c).unwrap();
    let mut s = initial();
    let context = v3::Context::new(&chart, &c).unwrap();
    for task in ts {
        let mut next = assign(&s, &task, Hand::R, &chart, &c, false).remove(0);
        context.update(&s, &mut next).unwrap();
        context.verify(&next).unwrap();
        s = next;
    }
    let actions = s.actions.to_vec();
    assert_eq!(actions.len(), 3);
    assert_eq!(actions[0].kind, ActionKind::Strike);
    assert_eq!(actions[1].kind, ActionKind::ContinuousContact);
    assert_eq!(actions[2].kind, ActionKind::ContinuousContact);
    assert_eq!(actions[0].note_id.as_deref(), Some("n0"));
    assert!(s.arms[1]
        .segments
        .to_vec()
        .iter()
        .any(|s| s.mode == "glide"));
    // Independent verification must detect corruption in an event-derived term.
    s.v3.parts.workload += 1.;
    assert_eq!(context.verify(&s).unwrap_err().code, "invalid_score");
}
#[test]
fn palm_is_one_action_and_extension_is_not_a_new_action() {
    let c = SolverConfig::v3();
    let chart = parse_chart("(120){8}Ch[4:2]/B1,B2,E", 0.).unwrap().chart;
    let ts = tasks(&chart, &c).unwrap();
    let group: Vec<_> = ts.iter().filter(|t| t.start == 0.).cloned().collect();
    let candidate = palm_candidates(&group, &chart, c.palm_radius)
        .into_iter()
        .find(|p| p.covered.len() == 2)
        .unwrap();
    let s = initial();
    let mut next = assign_palm(&s, &candidate, &group, Hand::R, &chart, &c, false).remove(0);
    let context = v3::Context::new(&chart, &c).unwrap();
    context.update(&s, &mut next).unwrap();
    context.verify(&next).unwrap();
    assert_eq!(next.actions.len, 1);
    assert_eq!(next.actions.last().unwrap().kind, ActionKind::Palm);
    assert_eq!(next.assignments.len, 2);
    let later = ts.iter().find(|t| t.start > 0.).unwrap();
    let mut extended = extend_palm_to_touch(&next, later, Hand::R, &chart, &c).unwrap();
    context.update(&next, &mut extended).unwrap();
    context.verify(&extended).unwrap();
    assert_eq!(extended.actions.len, 1);
    assert_eq!(extended.assignments.len, 3);
}
#[test]
fn slide_tracking_pickup_brush_and_duplicate_contact_add_no_action() {
    let c = SolverConfig::v3();
    let chart = parse_chart("(120){4}1-5[4:1],{16},E2,E", 0.).unwrap().chart;
    let ts = tasks(&chart, &c).unwrap();
    let context = v3::Context::new(&chart, &c).unwrap();
    let mut s = initial();
    for task in ts.iter().filter(|t| t.note == 0) {
        let mut next = assign(&s, task, Hand::R, &chart, &c, false).remove(0);
        context.update(&s, &mut next).unwrap();
        context.verify(&next).unwrap();
        s = next;
    }
    assert_eq!(s.actions.len, 1);
    // Only the slide head is a genuine contact: checkpoints never observe ownership.
    let owned = |s: &State| {
        s.v3.roles
            .ownership
            .entries()
            .map(|(k, e)| (k, e.owner))
            .collect::<Vec<_>>()
    };
    assert_eq!(owned(&s), [(ContactKey::Button(1), Hand::R)]);
    let touch = ts.iter().find(|t| t.note == 1).unwrap();
    let mut brushed = brush_touch(&s, touch, Hand::R, &chart, &c).unwrap();
    context.update(&s, &mut brushed).unwrap();
    context.verify(&brushed).unwrap();
    assert_eq!(brushed.actions.len, 1);
    assert_eq!(owned(&brushed), owned(&s));
    let chart = parse_chart("(120){4}1/1,E", 0.).unwrap().chart;
    let ts = tasks(&chart, &c).unwrap();
    let mut s = initial();
    for t in &ts {
        s = assign(&s, t, Hand::R, &chart, &c, false).remove(0);
    }
    assert_eq!(s.assignments.len, 2);
    assert_eq!(s.actions.len, 1);
}
#[test]
fn screen_sweep_records_continuous_contacts_in_each_hands_time_order() {
    let c = SolverConfig::v3();
    let source = format!(
        "(120){{4}}{},E",
        ["A", "B", "D", "E"]
            .into_iter()
            .flat_map(|area| (1..=8).map(move |i| format!("{area}{i}")))
            .chain(["C".into()])
            .collect::<Vec<_>>()
            .join("/")
    );
    let chart = parse_chart(&source, 0.).unwrap().chart;
    let ts = tasks(&chart, &c).unwrap();
    let s = initial();
    let mut next = assign_touch_sweep(&s, &ts, &chart, &c, false).unwrap();
    let context = v3::Context::new(&chart, &c).unwrap();
    context.update(&s, &mut next).unwrap();
    context.verify(&next).unwrap();
    assert_eq!(next.actions.len, 33);
    for hand in [Hand::L, Hand::R] {
        let events: Vec<_> = next
            .actions
            .to_vec()
            .into_iter()
            .filter(|e| e.hand == hand)
            .collect();
        assert!(events
            .iter()
            .all(|e| e.kind == ActionKind::ContinuousContact));
        assert!(events
            .windows(2)
            .all(|p| p[1].time_seconds >= p[0].time_seconds));
    }
    assert_eq!(next.v3.parts.jack_fatigue, 0.);
}

fn placed(l: u8, r: u8, travel: f64) -> State {
    let mut s = initial();
    s.arms[0].point = button(l);
    s.arms[1].point = button(r);
    s.v3.parts.travel = travel;
    s
}
#[test]
fn future_role_changes_rank_but_never_the_reported_score() {
    let c = SolverConfig::v3();
    let chart = parse_chart("(120){8}13,{16}2,2,{8}3,1,E", 0.)
        .unwrap()
        .chart;
    let context = v3::Context::new(&chart, &c).unwrap();
    // A: slightly cheaper now, both hands crowd the middle of the 1/3 phrase.
    // B: slightly more expensive now, hands already on the two anchors.
    let mut states = vec![placed(2, 2, 0.10), placed(1, 3, 0.12)];
    context.plan(&mut states, -0.05).unwrap();
    let (a, b) = (&states[0], &states[1]);
    assert!(a.v3.future.anchor_readiness > 0.);
    assert_eq!(b.v3.future.anchor_readiness, 0.);
    let rank = |s: &State| s.v3.rank_score.as_ref().unwrap().total();
    let score = |s: &State| s.v3.score.as_ref().unwrap().total();
    assert!(rank(b) < rank(a), "{} / {}", rank(b), rank(a));
    assert!(score(a) < score(b));
    for s in &states {
        assert!((score(s) - s.v3.parts.values().iter().sum::<f64>()).abs() < 1e-12);
        assert!((rank(s) - score(s) - s.v3.future.total()).abs() < 1e-12);
    }
    assert_eq!(v3::compare(a, b, true), std::cmp::Ordering::Less);
    let mut sorted = states.clone();
    sorted.sort_by(|x, y| v3::compare(x, y, false));
    assert_eq!(sorted[0].arms[0].point.distance(button(1)), 0.);
}
#[test]
fn return_readiness_keeps_the_helper_that_can_leave_the_cluster() {
    let c = SolverConfig::v3();
    let chart = parse_chart("(150){16}7,6,7,6,7,{8}2,E", 0.).unwrap().chart;
    let context = v3::Context::new(&chart, &c).unwrap();
    // After the last 6: the phrase finishes on 7, then the right side needs 2.
    let time = chart.notes[3].time_seconds;
    let mut states = vec![placed(7, 6, 0.), placed(7, 3, 0.)];
    context.plan(&mut states, time).unwrap();
    let (stuck, ready) = (&states[0], &states[1]);
    assert!(
        ready.v3.future.return_readiness < stuck.v3.future.return_readiness,
        "{:?} / {:?}",
        ready.v3.future,
        stuck.v3.future
    );
    assert_eq!(v3::compare(ready, stuck, false), std::cmp::Ordering::Less);
    assert_eq!(v3::compare(ready, stuck, true), std::cmp::Ordering::Equal);
}
#[test]
fn lookahead_window_is_bounded_by_groups_and_seconds() {
    let chart = parse_chart("(240){32}1,2,3,4,5,6,7,8,1,2,3,4,E", 0.)
        .unwrap()
        .chart;
    let look = v3::lookahead::Lookahead::from_chart(&chart);
    let w = look.window(-1e-3);
    assert_eq!(w.len(), v3::lookahead::LOOKAHEAD_MAX_GROUPS);
    let slow = parse_chart("(60){4}1,2,3,4,E", 0.).unwrap().chart;
    let look = v3::lookahead::Lookahead::from_chart(&slow);
    // 1 second apart: only the next onset within 0.8 s is visible.
    assert_eq!(look.window(0.5).len(), 1);
    assert_eq!(look.window(0.1).len(), 0);
    assert_eq!(look.revisit(ContactKey::Button(2), 0.5), Some(1.0));
}
