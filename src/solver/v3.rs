//! Incremental V3 bookkeeping around the existing feasibility transitions.
use super::*;
use crate::scoring_v3::{
    HandHistory, RoleState, ScoreBreakdownV3 as ScoreBreakdown, ScoreV3 as Score, ScoringV3,
};
pub(super) mod lookahead;
use lookahead::{FutureParts, Lookahead};

#[derive(Clone, Default)]
pub(super) struct Data {
    histories: [HandHistory; 2],
    pub parts: ScoreBreakdown,
    pub score: Option<Score>,
    pub rank_score: Option<Score>,
    contacts_at: Option<f64>,
    contacts: [Vec<(Point, bool)>; 2],
    /// Short-term key ownership and temporary local roles (replayable from actions).
    pub roles: RoleState,
    /// Rank-only lookahead bias for the current group; never in `score`.
    pub future: FutureParts,
    /// Rank-only swapped posture a waiting hand has accrued up to the current
    /// group but that is only charged once its next segment is added.
    pub pending_posture: f64,
}

impl Data {
    /// 影響之後成本的 V3 歷史（手部負荷、連打、分工角色、同組接觸點）。
    pub fn fingerprint<H: std::hash::Hasher>(&self, h: &mut H) {
        use std::hash::Hash;
        let q = |x: f64| (x * 1e6).round() as i64;
        for history in &self.histories {
            history.fingerprint(h);
        }
        self.roles.fingerprint(h);
        self.contacts_at.map(q).hash(h);
        for contacts in &self.contacts {
            for (p, touch) in contacts {
                (q(p.x), q(p.y), *touch).hash(h);
            }
            contacts.len().hash(h);
        }
    }
}

pub(super) struct Context<'a> {
    engine: ScoringV3,
    repetition_seconds: f64,
    notes: BTreeMap<&'a str, &'a Note>,
    // Nominal tracking strain per unit of path length.
    nominal: BTreeMap<&'a str, f64>,
    lookahead: Lookahead,
    /// End of the last note; swapped posture is not charged after it.
    posture_end: f64,
}

fn error(message: String) -> Diagnostic {
    Diagnostic::plain("invalid_score", message)
}

impl<'a> Context<'a> {
    pub fn new(chart: &'a Chart, c: &SolverConfig) -> Result<Self, Diagnostic> {
        let engine = ScoringV3::new(c.preferences_v3()).map_err(error)?;
        let mut nominal = BTreeMap::new();
        for note in &chart.notes {
            if let Some(id) = &note.path_id {
                let path = chart.paths.iter().find(|p| &p.id == id).unwrap();
                let distance: f64 = path
                    .samples
                    .windows(2)
                    .map(|p| {
                        Point {
                            x: p[0].x,
                            y: p[0].y,
                        }
                        .distance(Point {
                            x: p[1].x,
                            y: p[1].y,
                        })
                    })
                    .sum();
                let seconds = note.motion_end.unwrap() - note.motion_start.unwrap();
                let burden = engine.movement_strain(distance, seconds).map_err(error)?;
                nominal.insert(
                    note.id.as_str(),
                    if distance > 0.0 {
                        burden / distance
                    } else {
                        0.0
                    },
                );
            }
        }
        Ok(Self {
            engine,
            repetition_seconds: c.repetition_seconds,
            notes: chart.notes.iter().map(|n| (n.id.as_str(), n)).collect(),
            nominal,
            lookahead: Lookahead::from_chart(chart),
            posture_end: chart
                .notes
                .iter()
                .map(|n| n.end_seconds.max(n.motion_end.unwrap_or(n.time_seconds)))
                .fold(f64::NEG_INFINITY, f64::max),
        })
    }

    /// Update only changed tails. A glide or palm replacement shares the older
    /// prefix, so its old contribution (including cross overlap) is removable.
    pub fn update(&self, old: &State, next: &mut State) -> Result<(), Diagnostic> {
        let mut diffs = Vec::new();
        for idx in 0..2 {
            let (removed, added, common) =
                changed(&old.arms[idx].segments, &next.arms[idx].segments);
            let hand = if idx == 0 { Hand::L } else { Hand::R };
            for (segments, sign) in [(&removed, -1.0), (&added, 1.0)] {
                for segment in segments {
                    let terms = self.engine.motion_terms(segment, hand).map_err(error)?;
                    next.v3.parts.excursion += sign * terms.excursion;
                    next.v3.parts.speed_strain += sign * terms.speed_strain;
                    next.v3.parts.travel += sign * terms.travel;
                    // The new owner carries mode=slide for the entire tracking
                    // interval. mode=handover is the duplicate old-hand overlap.
                    if segment.mode == "slide" {
                        let distance: f64 = segment
                            .samples
                            .windows(2)
                            .map(|p| p[0].point().distance(p[1].point()))
                            .sum();
                        let rate = self.nominal[segment.note_id.as_deref().unwrap()];
                        next.v3.parts.compression_strain += sign
                            * self
                                .engine
                                .compression(terms.tracking_strain, rate * distance)
                                .map_err(error)?;
                    }
                }
            }
            diffs.push((removed, added, common));
        }
        // new L x new R - old L x old R, without counting changed pairs twice.
        for segment in &diffs[0].0 {
            next.v3.parts.cross_exposure -=
                self.cross_chain(segment, &old.arms[1].segments, true)?;
        }
        for segment in &diffs[0].1 {
            next.v3.parts.cross_exposure +=
                self.cross_chain(segment, &next.arms[1].segments, true)?;
        }
        for segment in &diffs[1].0 {
            next.v3.parts.cross_exposure -= self.cross_chain(segment, &diffs[0].2, false)?;
        }
        for segment in &diffs[1].1 {
            next.v3.parts.cross_exposure += self.cross_chain(segment, &diffs[0].2, false)?;
        }

        let mut fresh = Vec::new();
        let mut cursor = &next.assignments;
        while cursor.len > old.assignments.len {
            let link = cursor.head.as_ref().unwrap();
            fresh.push(&link.value);
            cursor = &link.prev;
        }
        fresh.reverse();
        for assignment in fresh {
            // Touch Group 連帶判定沒有實際接觸，不計分工。
            if assignment.part == "slide" || assignment.part == "group" {
                continue;
            }
            let note = self.notes[assignment.note_id.as_str()];
            // Use semantic chart time, not an early sweep's contact timestamp.
            if next.v3.contacts_at != Some(note.time_seconds) {
                next.v3.contacts_at = Some(note.time_seconds);
                next.v3.contacts = [vec![], vec![]];
            }
            let points = &mut next.v3.contacts[assignment.hand.index()];

            let touch = matches!(note.kind.as_str(), "touch" | "touchHold");
            if !points.iter().any(|(p, _)| p.distance(note.position) <= EPS) {
                points.push((note.position, touch));
            }
        }
        for event in fresh_values(&old.actions, &next.actions) {
            let data = &mut next.v3;
            let terms = self.action(&mut data.histories, &mut data.roles, event)?;
            data.parts.jack_fatigue += terms.jack_fatigue;
            data.parts.reversal += terms.reversal;
            data.parts.workload += terms.workload;
            data.parts.ownership_switch += terms.ownership_switch;
        }
        for handover in fresh_values(&old.handovers, &next.handovers) {
            next.v3.parts.handover += self.engine.handover(handover.swap);
        }
        self.refresh(next)
    }

    /// One physical action with V3.1 ownership and roles. Only genuine button
    /// contacts (strike or continuous arrival) observe ownership; palms and
    /// Touch contacts release the hand's local role. Slide checkpoints,
    /// handovers, brushes and Touch Group completion have no action at all.
    fn action(
        &self,
        histories: &mut [HandHistory; 2],
        roles: &mut RoleState,
        event: &ActionEvent,
    ) -> Result<ScoreBreakdown, Diagnostic> {
        let (hand, point, time) = (event.hand, event.point, event.time_seconds);
        let key = event
            .note_id
            .as_deref()
            .map(|id| lookahead::contact_key(self.notes[id]))
            .filter(|key| {
                event.kind != ActionKind::Palm
                    && matches!(key, crate::scoring_v3::ContactKey::Button(_))
            });
        let history = &mut histories[hand.index()];
        let (mut switch, mut scale) = (0.0, 1.0);
        if let Some(key) = key {
            let chord = self.lookahead.is_button_chord(time);
            switch = roles.contact_cost(hand, point, key, time, chord);
            let recurs = history
                .recent_points()
                .1
                .is_some_and(|p| self.lookahead.point_recurs(p, time));
            scale = roles.reversal_scale(history, hand, point, time, recurs);
        }
        let mut terms = self
            .engine
            .action_with(history, event, self.repetition_seconds, scale)
            .map_err(error)?;
        match key {
            Some(key) => roles.observe(hand, point, key, self.lookahead.revisit(key, time), time),
            None => roles.release(hand),
        }
        terms.ownership_switch = switch;
        Ok(terms)
    }

    /// Rank-only lookahead for every state that finished the group at `time`.
    pub fn plan(&self, states: &mut [State], time: f64) -> Result<(), Diagnostic> {
        let window = self.lookahead.window(time);
        for state in states {
            let hands = [state.arms[0].point, state.arms[1].point];
            state.v3.future = lookahead::future_role(window, hands, &state.v3.roles);
            state.v3.pending_posture = self.pending_posture(state, time);
            self.refresh(state)?;
        }
        Ok(())
    }

    /// Posture while a hand waits past its last segment, assuming it stays put.
    fn pending_posture(&self, state: &State, time: f64) -> f64 {
        let [l, r] = &state.arms;
        let from = l.free.min(r.free);
        let to = time.min(self.posture_end);
        if to <= from {
            return 0.0;
        }
        let at = |arm: &Arm, t: f64| {
            if t < arm.free {
                arm_point_at(arm, t)
            } else {
                arm.point
            }
        };
        const PIECES: usize = 8;
        let h = (to - from) / PIECES as f64;
        let f = |t: f64| self.engine.swapped_rate(at(l, t), at(r, t));
        (0..PIECES)
            .map(|k| {
                let a = from + h * k as f64;
                h / 6.0 * (f(a) + 4.0 * f(a + h / 2.0) + f(a + h))
            })
            .sum()
    }

    /// Debug replay of ownership, roles and the lookahead parts per action.
    pub fn trace(&self, state: &State) -> Result<Vec<String>, Diagnostic> {
        let mut histories = [HandHistory::default(), HandHistory::default()];
        let mut roles = RoleState::default();
        let mut hands = [state.arms[0].point; 2];
        let mut lines = vec![];
        for event in state.actions.to_vec() {
            let terms = self.action(&mut histories, &mut roles, &event)?;
            hands[event.hand.index()] = event.point;
            let t = event.time_seconds;
            let future = lookahead::future_role(self.lookahead.window(t), hands, &roles);
            let ownership: Vec<String> = roles
                .ownership
                .entries()
                .filter_map(|(key, e)| {
                    let s = e.strength_at(t);
                    (s > 0.0).then(|| format!("{key}->{:?} {s:.2}", e.owner))
                })
                .collect();
            let role = |h: Hand| {
                let r = roles.roles[h.index()];
                match r.anchor {
                    Some(a) if r.live(t) => {
                        format!(
                            "{h:?} anchor {a} until {:.3} ({:.2})",
                            r.expires_at, r.strength
                        )
                    }
                    _ => format!("{h:?} free"),
                }
            };
            lines.push(format!(
                "{t:.3} {} {:?} {:?} switch {:.3} reversal {:.3} | ownership: {} | roles: {}; {} | future: ownership {:.3} anchor {:.3} return {:.3}",
                event.note_id.as_deref().unwrap_or("-"),
                event.hand,
                event.kind,
                terms.ownership_switch,
                terms.reversal,
                ownership.join(", "),
                role(Hand::L),
                role(Hand::R),
                future.ownership_readiness,
                future.anchor_readiness,
                future.return_readiness,
            ));
        }
        Ok(lines)
    }

    fn cross_chain(
        &self,
        segment: &MotionSegment,
        chain: &Chain<MotionSegment>,
        is_left: bool,
    ) -> Result<f64, Diagnostic> {
        let mut cursor = chain.head.as_ref();
        let mut result = 0.0;
        while let Some(link) = cursor {
            let other = &link.value;
            if other.end_seconds <= segment.start_seconds {
                break;
            }
            if other.start_seconds < segment.end_seconds {
                result += if is_left {
                    self.engine.crossing_until(segment, other, self.posture_end)
                } else {
                    self.engine.crossing_until(other, segment, self.posture_end)
                }
                .map_err(error)?;
            }
            cursor = link.prev.head.as_ref();
        }
        Ok(result)
    }

    pub fn refresh(&self, state: &mut State) -> Result<(), Diagnostic> {
        // Remove cancellation noise only. Real negative values remain errors.
        let p = &mut state.v3.parts;
        for v in [
            &mut p.excursion,
            &mut p.cross_exposure,
            &mut p.speed_strain,
            &mut p.compression_strain,
            &mut p.travel,
        ] {
            if *v < 0.0 && *v > -1e-9 {
                *v = 0.0;
            }
        }
        state.v3.score = Some(self.engine.score(p).map_err(error)?);
        let mut rank = p.clone();
        // Rank-only terms: deferred slide deposit and the lookahead role bias.
        rank.excursion +=
            state.pending.max(0.0) + state.v3.future.total() + state.v3.pending_posture;
        state.v3.rank_score = Some(self.engine.score(&rank).map_err(error)?);
        Ok(())
    }

    /// Deferred path progress gets the smaller home-side exposure of the two
    /// hands. It is an optimistic ranking deposit, never a reported cost.
    pub fn deposit(&self, task: &Task, chart: &Chart) -> Result<f64, Diagnostic> {
        let note = &chart.notes[task.note];
        let samples = path_samples(
            &chart.paths[task.path],
            note.motion_start.unwrap(),
            note.motion_end.unwrap(),
            task.start,
            task.end,
        );
        let segment = MotionSegment {
            mode: "slide".into(),
            note_id: Some(note.id.clone()),
            start_seconds: task.start,
            end_seconds: task.end,
            samples,
        };
        let left = self
            .engine
            .motion_terms(&segment, Hand::L)
            .map_err(error)?
            .excursion;
        let right = self
            .engine
            .motion_terms(&segment, Hand::R)
            .map_err(error)?
            .excursion;
        Ok(left.min(right))
    }

    /// Independent complete trajectory recomputation for the final candidates.
    /// Replays physical actions and handovers as well as removable motion terms,
    /// so missing/duplicated incremental updates fail before publishing a score.
    pub fn verify(&self, state: &State) -> Result<(), Diagnostic> {
        let mut expected = ScoreBreakdown::default();
        let left = state.arms[0].segments.to_vec();
        let right = state.arms[1].segments.to_vec();
        for (segments, hand) in [(&left, Hand::L), (&right, Hand::R)] {
            for segment in segments {
                let t = self.engine.motion_terms(segment, hand).map_err(error)?;
                expected.excursion += t.excursion;
                expected.speed_strain += t.speed_strain;
                expected.travel += t.travel;
                if segment.mode == "slide" {
                    let distance: f64 = segment
                        .samples
                        .windows(2)
                        .map(|p| p[0].point().distance(p[1].point()))
                        .sum();
                    expected.compression_strain += self
                        .engine
                        .compression(
                            t.tracking_strain,
                            self.nominal[segment.note_id.as_deref().unwrap()] * distance,
                        )
                        .map_err(error)?;
                }
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < left.len() && j < right.len() {
            expected.cross_exposure += self
                .engine
                .crossing_until(&left[i], &right[j], self.posture_end)
                .map_err(error)?;
            let (le, re) = (left[i].end_seconds, right[j].end_seconds);
            if le <= re {
                i += 1;
            }
            if re <= le {
                j += 1;
            }
        }
        let mut histories = [HandHistory::default(), HandHistory::default()];
        let mut roles = RoleState::default();
        for event in state.actions.to_vec() {
            let terms = self.action(&mut histories, &mut roles, &event)?;
            expected.jack_fatigue += terms.jack_fatigue;
            expected.reversal += terms.reversal;
            expected.workload += terms.workload;
            expected.ownership_switch += terms.ownership_switch;
        }
        for handover in state.handovers.to_vec() {
            expected.handover += self.engine.handover(handover.swap);
        }
        let actual = &state.v3.parts;
        for (a, b) in expected.values().into_iter().zip(actual.values()) {
            if (a - b).abs() > 1e-7 * (1.0 + a.abs()) {
                return Err(error(format!("V3 增量成本不一致：{a} / {b}")));
            }
        }
        Ok(())
    }
}

fn changed<T: Clone>(old: &Chain<T>, new: &Chain<T>) -> (Vec<T>, Vec<T>, Chain<T>) {
    let (mut a, mut b) = (old, new);
    let (mut removed, mut added) = (vec![], vec![]);
    loop {
        if a.len == b.len
            && match (&a.head, &b.head) {
                (None, None) => true,
                (Some(x), Some(y)) => Arc::ptr_eq(x, y),
                _ => false,
            }
        {
            break;
        }
        if a.len >= b.len && a.len > 0 {
            let l = a.head.as_ref().unwrap();
            removed.push(l.value.clone());
            a = &l.prev;
        } else {
            let l = b.head.as_ref().unwrap();
            added.push(l.value.clone());
            b = &l.prev;
        }
    }
    added.reverse();
    (removed, added, a.clone())
}

/// Complete the simultaneous group before deciding whether pickup conflicts.
pub(super) fn remove_unnecessary_deferrals(states: &mut Vec<State>) {
    let key = |s: &State| {
        let mut contacts: Vec<_> =
            s.v3.contacts
                .iter()
                .enumerate()
                .flat_map(|(hand, points)| {
                    points
                        .iter()
                        .map(move |(p, touch)| (hand, p.x.to_bits(), p.y.to_bits(), *touch))
                })
                .collect();
        contacts.sort();
        (s.group_origin, contacts)
    };
    let ready: std::collections::BTreeSet<_> = states
        .iter()
        .filter(|s| s.deposit.is_empty())
        .map(&key)
        .collect();
    states.retain(|s| s.deposit.is_empty() || !ready.contains(&key(s)));
}

pub(super) fn compare(a: &State, b: &State, final_rank: bool) -> std::cmp::Ordering {
    let left = if final_rank {
        &a.v3.score
    } else {
        &a.v3.rank_score
    };
    let right = if final_rank {
        &b.v3.score
    } else {
        &b.v3.rank_score
    };
    left.as_ref()
        .unwrap()
        .compare(right.as_ref().unwrap())
        .then_with(|| a.deposit.len().cmp(&b.deposit.len()))
}

fn fresh_values<'a, T>(old: &Chain<T>, new: &'a Chain<T>) -> Vec<&'a T> {
    let mut cursor = new;
    let mut result = Vec::new();
    while cursor.len > old.len {
        let link = cursor.head.as_ref().unwrap();
        result.push(&link.value);
        cursor = &link.prev;
    }
    result.reverse();
    result
}
