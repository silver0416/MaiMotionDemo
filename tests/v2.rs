use mai_motion_core::{scoring::*, *};

fn engine() -> ScoringV2 {
    ScoringV2::new(PreferenceConfig::default()).unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-7, "{a} != {b}");
}
fn segment(mode: &str, samples: &[(f64, f64)]) -> MotionSegment {
    MotionSegment {
        mode: mode.into(),
        note_id: None,
        start_seconds: samples[0].0,
        end_seconds: samples.last().unwrap().0,
        samples: samples
            .iter()
            .map(|(t, x)| MotionSample::new(*t, Point { x: *x, y: 0.0 }))
            .collect(),
    }
}
fn run(source: &str, solver_config: SolverConfig) -> AnalyzeResponse {
    analyze_chart(AnalyzeRequest {
        request_id: "v2".into(),
        source: source.into(),
        first_seconds: 0.0,
        solver_config,
    })
}
fn ok(source: &str) -> AnalyzeResponse {
    let r = run(source, SolverConfig::v2());
    assert_eq!(r.status, "ok", "{source}: {:?}", r.diagnostics);
    r
}
fn continuity(segments: &[MotionSegment]) {
    for pair in segments.windows(2) {
        near(pair[0].end_seconds, pair[1].start_seconds);
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
    for s in segments {
        for p in s.samples.windows(2) {
            assert!(p[1].time_seconds >= p[0].time_seconds);
            if p[1].time_seconds == p[0].time_seconds {
                assert_eq!(p[0].point(), p[1].point());
            }
        }
    }
}

#[test]
fn preferences_validate_boundaries_and_reject_legacy_keys() {
    for value in [-1.0, 101.0, f64::NAN, f64::INFINITY] {
        assert!(ScoringV2::new(PreferenceConfig {
            home_preference: value,
            ..Default::default()
        })
        .is_err());
    }
    for value in [1.0, 30.0, 120.0, 200.0] {
        assert!(ScoringV2::new(PreferenceConfig {
            travel_comfort: value,
            ..Default::default()
        })
        .is_ok());
    }
    for json in [
        r#"{"scoringModel":"hand-affinity-v2","sideWeight":1}"#,
        r#"{"homePreference":80}"#,
        r#"{"scoringModel":"wrong"}"#,
        r#"{"scoringModel":"hand-affinity-v2","extra":1}"#,
    ] {
        assert!(
            serde_json::from_str::<SolverConfig>(json).is_err(),
            "{json}"
        );
    }
    let config: SolverConfig =
        serde_json::from_str(r#"{"scoringModel":"hand-affinity-v2"}"#).unwrap();
    assert!(config.is_v2());
    let json = serde_json::to_value(config).unwrap();
    assert!(json.get("sideWeight").is_none());
    assert!(json.get("speedReference").is_none());
    assert_eq!(json["travelComfort"], 30.0);
    assert!(serde_json::to_value(SolverConfig::default())
        .unwrap()
        .get("homePreference")
        .is_none());
}

#[test]
fn affinity_is_symmetric_with_a_neutral_zone() {
    for x in [-1.0, -0.4, -0.15, 0.0, 0.15, 0.4, 1.0] {
        near(
            engine().affinity(Hand::L, Point { x, y: 0.0 }).unwrap(),
            engine().affinity(Hand::R, Point { x: -x, y: 0.0 }).unwrap(),
        );
    }
    near(
        engine()
            .affinity(Hand::L, Point { x: 0.15, y: 0.8 })
            .unwrap(),
        0.0,
    );
    near(
        engine()
            .affinity(Hand::L, Point { x: 1.0, y: 0.0 })
            .unwrap(),
        1.0,
    );
}

#[test]
fn fast_motion_is_soft_finite_and_partition_invariant() {
    let e = engine();
    near(e.movement_strain(6.0, 0.05).unwrap(), 0.25);
    near(e.movement_strain(1.0, 0.1).unwrap(), 0.0);
    near(
        e.movement_strain(6.0, 0.05).unwrap(),
        2.0 * e.movement_strain(3.0, 0.025).unwrap(),
    );
    assert!(e.movement_strain(1.0, 0.0).is_err());
    near(e.movement_strain(0.0, 0.0).unwrap(), 0.0);
    for speed in [30.0, 60.0, 120.0, 180.0] {
        let burden = e.movement_strain(speed * 0.05, 0.05).unwrap();
        assert!(burden.is_finite());
        let relaxed = ScoringV2::new(PreferenceConfig {
            travel_comfort: 120.0,
            ..Default::default()
        })
        .unwrap();
        assert!(relaxed.movement_strain(speed * 0.05, 0.05).unwrap() <= burden);
    }
}

#[test]
fn posture_uses_exact_shared_time_and_excludes_free_travel() {
    let l = segment("hold", &[(0.0, -1.0), (2.0, 1.0)]);
    let r = segment("hold", &[(0.0, 0.0), (2.0, 0.0)]);
    near(engine().crossing(&l, &r).unwrap(), 1.0 / 12.0);
    let divided = segment("hold", &[(0.0, -1.0), (0.5, -0.5), (1.0, 0.0), (2.0, 1.0)]);
    near(engine().crossing(&divided, &r).unwrap(), 1.0 / 12.0);
    near(
        engine()
            .crossing(&l, &segment("hold", &[(2.0, -1.0), (3.0, -1.0)]))
            .unwrap(),
        0.0,
    );
    near(
        engine()
            .crossing(&l, &segment("travel", &[(0.0, -1.0), (2.0, -1.0)]))
            .unwrap(),
        0.0,
    );
    near(
        engine().motion_terms(&l, Hand::L).unwrap().side_exposure,
        engine()
            .motion_terms(&divided, Hand::L)
            .unwrap()
            .side_exposure,
    );
    let invalid = segment("travel", &[(0.0, 0.0), (0.0, 1.0)]);
    assert!(engine().motion_terms(&invalid, Hand::L).is_err());
}

#[test]
fn required_tracking_is_separate_from_compression_and_restrikes() {
    let s = segment("slide", &[(0.0, -1.0), (0.01, 1.0)]);
    let terms = engine().motion_terms(&s, Hand::L).unwrap();
    assert!(terms.tracking_strain > 0.0);
    near(terms.travel_strain, 0.0);
    near(terms.free_distance, 0.0);
    near(
        engine()
            .compression(terms.tracking_strain, terms.tracking_strain)
            .unwrap(),
        0.0,
    );
    near(engine().compression(0.1, 0.2).unwrap(), 0.0);
    assert!(engine().repetition(0.01, 0.15).unwrap() > 0.0);
    let e = ScoringV2::new(PreferenceConfig {
        repeat_tolerance: 100.0,
        handover_willingness: 100.0,
        ..Default::default()
    })
    .unwrap();
    near(e.repetition(0.01, 0.15).unwrap(), 0.0);
    near(e.handover(false), 0.1);
    near(e.handover(true), 0.0);
}

#[test]
fn home_preference_changes_tradeoff_and_distance_is_only_tiebreaker() {
    let natural = ScoreBreakdown {
        travel_strain: 1.0,
        free_distance: 100.0,
        ..Default::default()
    };
    let crossed = ScoreBreakdown {
        assignment_affinity: 0.5,
        free_distance: 0.0,
        ..Default::default()
    };
    let e = engine();
    assert!(e
        .score(&natural)
        .unwrap()
        .compare(&e.score(&crossed).unwrap())
        .is_lt());
    let relaxed = ScoringV2::new(PreferenceConfig {
        home_preference: 0.0,
        ..Default::default()
    })
    .unwrap();
    assert!(relaxed
        .score(&natural)
        .unwrap()
        .compare(&relaxed.score(&crossed).unwrap())
        .is_gt());
    let extreme = ScoreBreakdown {
        travel_strain: 100.0,
        ..natural
    };
    assert!(e
        .score(&extreme)
        .unwrap()
        .compare(&e.score(&crossed).unwrap())
        .is_gt());
    assert!(e
        .score(&ScoreBreakdown {
            travel_strain: f64::NAN,
            ..Default::default()
        })
        .is_err());
    let values = [0.0, 0.49e-6, 0.51e-6, 0.99e-6, 1.01e-6, 1.49e-6];
    let scores: Vec<_> = values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            e.score(&ScoreBreakdown {
                assignment_affinity: *v,
                free_distance: (6 - i) as f64,
                ..Default::default()
            })
            .unwrap()
        })
        .collect();
    for a in &scores {
        for b in &scores {
            for c in &scores {
                if a.compare(b).is_le() && b.compare(c).is_le() {
                    assert!(a.compare(c).is_le());
                }
            }
        }
    }
}

#[test]
fn v2_corpus_preserves_feasibility_continuity_and_wire_contract() {
    let cases = [
        "(120){4}1,2,3,4,E",
        "(120){4}8,7,6,5,E",
        "(120){4}1/5,E",
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
        assert_eq!(r.schema_version, 3);
        for solution in &r.solutions {
            continuity(&solution.left_segments);
            continuity(&solution.right_segments);
            let p = solution.score_breakdown.as_ref().unwrap();
            let score = solution.score.as_ref().unwrap();
            near(
                score.intuition(),
                p.assignment_affinity + p.side_exposure + p.cross_exposure + p.handover,
            );
            near(
                score.strain(),
                p.travel_strain + p.compression_strain + p.repetition,
            );
            near(
                score.ranking_value(),
                score.intuition() + engine().strain_weight() * score.strain(),
            );
        }
        let json = serde_json::to_value(&r).unwrap();
        assert!(json["solutions"][0].get("totalCost").is_none());
        assert!(json["solutions"][0].get("costBreakdown").is_none());
        let decoded: AnalyzeResponse = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.solutions.len(), r.solutions.len());
    }
}

#[test]
fn default_v2_uses_home_hands_but_keeps_forced_cross_side() {
    let r = ok("(120){4}1,8,2,7,3,6,4,5,E");
    let chart = r.chart.as_ref().unwrap();
    for a in &r.solutions[0].assignments {
        let note = chart.notes.iter().find(|n| n.id == a.note_id).unwrap();
        assert_eq!(
            a.hand,
            if note.position.x < 0.0 {
                Hand::L
            } else {
                Hand::R
            }
        );
    }
    let r = ok("(120){4}7h[4:3],6,5,E");
    let s = &r.solutions[0];
    assert_eq!(
        s.assignments
            .iter()
            .find(|a| a.note_id == "n0")
            .unwrap()
            .hand,
        Hand::L
    );
    assert!(s
        .assignments
        .iter()
        .any(|a| a.note_id == "n1" && a.hand == Hand::R));
    assert_eq!(
        run("(120){4}1/4/7,E", SolverConfig::v2()).status,
        "no_solution"
    );
}

#[test]
fn uncontested_pickup_is_on_time_and_meeting_swap_is_free() {
    let r = ok("(120){4}1-5[4:3],8,7,6,E");
    let pickup = r.solutions[0]
        .assignments
        .iter()
        .filter(|a| a.note_id == "n0" && a.part == "slide")
        .map(|a| a.start_seconds)
        .fold(f64::INFINITY, f64::min);
    near(pickup, 0.5);
    let r = ok("(120){4}1-5[4:1]/5-1[4:1],E");
    assert_eq!(
        r.solutions[0].handovers.iter().filter(|h| h.swap).count(),
        2
    );
    near(
        r.solutions[0].score_breakdown.as_ref().unwrap().handover,
        0.0,
    );
    let r = run(
        "(120){4}1-5[4:1]/5-1[4:1],E",
        SolverConfig {
            allow_handover: false,
            ..SolverConfig::v2()
        },
    );
    assert_eq!(r.status, "ok");
    assert!(r.solutions[0].handovers.is_empty());
}

#[test]
fn simultaneous_order_and_repeated_runs_preserve_scores() {
    let a = ok("(120){4}C/B1/E1/7,E");
    let b = ok("(120){4}7/E1/B1/C,E");
    near(
        a.solutions[0].score.as_ref().unwrap().ranking_value(),
        b.solutions[0].score.as_ref().unwrap().ranking_value(),
    );
    assert_eq!(
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&ok("(120){4}C/B1/E1/7,E")).unwrap()
    );
}

#[test]
fn small_tap_search_matches_exhaustive_assignment_oracle() {
    // Independent enumeration of all 16 hand sequences. No Slide, glide or
    // overlapping contacts: compute feasibility and scores directly here.
    for home in [0.0, 80.0, 100.0] {
        let config = SolverConfig {
            home_preference: home,
            glide_distance: 0.0,
            beam_width: 256,
            ..SolverConfig::v2()
        };
        let r = run("(240){8}1,8,3,6,E", config.clone());
        assert_eq!(r.status, "ok");
        let notes = &r.chart.as_ref().unwrap().notes;
        let e = ScoringV2::new(config.preferences()).unwrap();
        let button = |k: f64| {
            let angle = -std::f64::consts::FRAC_PI_2
                + std::f64::consts::PI / 8.0
                + (k - 1.0) * std::f64::consts::FRAC_PI_4;
            Point {
                x: angle.cos(),
                y: angle.sin(),
            }
        };
        let mut best: Option<Score> = None;
        for mask in 0..16 {
            let mut positions = [button(7.0), button(2.0)];
            let mut free = [-1.0, -1.0];
            let mut last: [Option<f64>; 2] = [None, None];
            let mut parts = scoring::ScoreBreakdown::default();
            for (i, n) in notes.iter().enumerate() {
                let hand = if mask & (1 << i) == 0 {
                    Hand::L
                } else {
                    Hand::R
                };
                let h = hand.index();
                let distance = positions[h].distance(n.position);
                parts.assignment_affinity += e.affinity(hand, n.position).unwrap();
                parts.travel_strain += e
                    .movement_strain(distance, n.time_seconds - free[h])
                    .unwrap();
                parts.free_distance += distance;
                if let Some(t) = last[h] {
                    parts.repetition += e
                        .repetition(n.time_seconds - t, config.repetition_seconds)
                        .unwrap();
                }
                positions[h] = n.position;
                free[h] = n.time_seconds + config.contact_seconds;
                last[h] = Some(n.time_seconds);
            }
            let score = e.score(&parts).unwrap();
            if best.as_ref().is_none_or(|b| score.compare(b).is_lt()) {
                best = Some(score);
            }
        }
        let actual = r.solutions[0].score.as_ref().unwrap();
        near(
            actual.ranking_value(),
            best.as_ref().unwrap().ranking_value(),
        );
        near(actual.efficiency(), best.unwrap().efficiency());
    }
}

#[test]
fn sweep_and_checkpoint_refinement_keep_v2_accounting_consistent() {
    let mut areas = vec!["C".to_string()];
    for area in ['A', 'B', 'D', 'E'] {
        for index in 1..=8 {
            areas.push(format!("{area}{index}"));
        }
    }
    let r = ok(&format!("(120){{4}}{},E", areas.join("/")));
    assert_eq!(r.solutions[0].assignments.len(), 33);
    continuity(&r.solutions[0].left_segments);
    continuity(&r.solutions[0].right_segments);
    let a = ok("(120){4}1?-5[4:1],E");
    let b = run(
        "(120){4}1?-5[4:1],E",
        SolverConfig {
            checkpoint_seconds: 0.025,
            handover_seconds: 0.02,
            allow_handover: false,
            ..SolverConfig::v2()
        },
    );
    assert_eq!(b.status, "ok");
    near(
        a.solutions[0].score.as_ref().unwrap().ranking_value(),
        b.solutions[0].score.as_ref().unwrap().ranking_value(),
    );
}

#[test]
fn late_pickup_retains_compression_burden() {
    let r = run(
        "(120){4}1-5[4:1],3/7,E",
        SolverConfig {
            travel_comfort: 1.0,
            ..SolverConfig::v2()
        },
    );
    assert_eq!(r.status, "ok");
    assert!(
        r.solutions[0]
            .score_breakdown
            .as_ref()
            .unwrap()
            .compression_strain
            > 0.0
    );
    assert!(r.solutions[0]
        .assignments
        .iter()
        .filter(|a| a.part == "slide")
        .all(|a| a.start_seconds > 0.5));
}
