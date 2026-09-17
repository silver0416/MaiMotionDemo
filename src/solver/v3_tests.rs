//! Transition audit: inspect physical events, not merely the reported score.
use super::*;
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
    let touch = ts.iter().find(|t| t.note == 1).unwrap();
    let mut brushed = brush_touch(&s, touch, Hand::R, &chart, &c).unwrap();
    context.update(&s, &mut brushed).unwrap();
    context.verify(&brushed).unwrap();
    assert_eq!(brushed.actions.len, 1);
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
