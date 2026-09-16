//! Incremental V2 bookkeeping around the existing feasibility transitions.
use super::*;
use crate::scoring::{Score, ScoreBreakdown, ScoringV2};

#[derive(Clone, Default)]
pub(super) struct Data {
    pub parts: ScoreBreakdown,
    pub score: Option<Score>,
    pub rank_score: Option<Score>,
    contacts_at: Option<f64>,
    contacts: [Vec<(Point, bool)>; 2],
}

pub(super) struct Context<'a> {
    engine: ScoringV2,
    notes: BTreeMap<&'a str, &'a Note>,
    // Nominal tracking strain per unit of path length.
    nominal: BTreeMap<&'a str, f64>,
}

fn error(message: String) -> Diagnostic {
    Diagnostic::plain("invalid_score", message)
}

impl<'a> Context<'a> {
    pub fn new(chart: &'a Chart, c: &SolverConfig) -> Result<Self, Diagnostic> {
        let engine = ScoringV2::new(c.preferences()).map_err(error)?;
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
            notes: chart.notes.iter().map(|n| (n.id.as_str(), n)).collect(),
            nominal,
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
                    next.v2.parts.side_exposure += sign * terms.side_exposure;
                    next.v2.parts.travel_strain += sign * terms.travel_strain;
                    next.v2.parts.free_distance += sign * terms.free_distance;
                    // The new owner carries mode=slide for the entire tracking
                    // interval. mode=handover is the duplicate old-hand overlap.
                    if segment.mode == "slide" {
                        let distance: f64 = segment
                            .samples
                            .windows(2)
                            .map(|p| p[0].point().distance(p[1].point()))
                            .sum();
                        let rate = self.nominal[segment.note_id.as_deref().unwrap()];
                        next.v2.parts.compression_strain += sign
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
            next.v2.parts.cross_exposure -=
                self.cross_chain(segment, &old.arms[1].segments, true)?;
        }
        for segment in &diffs[0].1 {
            next.v2.parts.cross_exposure +=
                self.cross_chain(segment, &next.arms[1].segments, true)?;
        }
        for segment in &diffs[1].0 {
            next.v2.parts.cross_exposure -= self.cross_chain(segment, &diffs[0].2, false)?;
        }
        for segment in &diffs[1].1 {
            next.v2.parts.cross_exposure += self.cross_chain(segment, &diffs[0].2, false)?;
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
            if assignment.part == "slide" {
                continue;
            }
            let note = self.notes[assignment.note_id.as_str()];
            // Use semantic chart time, not an early sweep's contact timestamp.
            if next.v2.contacts_at != Some(note.time_seconds) {
                next.v2.contacts_at = Some(note.time_seconds);
                next.v2.contacts = [vec![], vec![]];
            }
            let points = &mut next.v2.contacts[assignment.hand.index()];
            let before = self.contact_cost(points, assignment.hand)?;
            let touch = matches!(note.kind.as_str(), "touch" | "touchHold");
            if !points.iter().any(|(p, _)| p.distance(note.position) <= EPS) {
                points.push((note.position, touch));
            }
            next.v2.parts.assignment_affinity +=
                self.contact_cost(points, assignment.hand)? - before;
        }
        // V2 configs require the legacy weights to stay at their internal unit
        // defaults. These two accumulators are therefore raw event counts/costs.
        next.v2.parts.repetition =
            2.0 * (1.0 - self.engine.config().repeat_tolerance / 100.0) * next.cost.repetition;
        next.v2.parts.handover = self.engine.handover(false) * next.cost.handover;
        self.refresh(next)
    }

    fn contact_cost(&self, points: &[(Point, bool)], hand: Hand) -> Result<f64, Diagnostic> {
        let (mut other, mut touch, mut count) = (0.0, 0.0, 0);
        for &(point, is_touch) in points {
            let cost = self.engine.affinity(hand, point).map_err(error)?;
            if is_touch {
                touch += cost;
                count += 1;
            } else {
                other += cost;
            }
        }
        Ok(other
            + if count == 0 {
                0.0
            } else {
                touch / count as f64
            })
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
                    self.engine.crossing(segment, other)
                } else {
                    self.engine.crossing(other, segment)
                }
                .map_err(error)?;
            }
            cursor = link.prev.head.as_ref();
        }
        Ok(result)
    }

    pub fn refresh(&self, state: &mut State) -> Result<(), Diagnostic> {
        // Remove cancellation noise only. Real negative values remain errors.
        let p = &mut state.v2.parts;
        for v in [
            &mut p.assignment_affinity,
            &mut p.side_exposure,
            &mut p.cross_exposure,
            &mut p.travel_strain,
            &mut p.compression_strain,
            &mut p.free_distance,
        ] {
            if *v < 0.0 && *v > -1e-9 {
                *v = 0.0;
            }
        }
        state.v2.score = Some(self.engine.score(p).map_err(error)?);
        let mut rank = p.clone();
        rank.side_exposure += state.pending.max(0.0);
        state.v2.rank_score = Some(self.engine.score(&rank).map_err(error)?);
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
            .side_exposure;
        let right = self
            .engine
            .motion_terms(&segment, Hand::R)
            .map_err(error)?
            .side_exposure;
        Ok(left.min(right))
    }

    /// Independent complete trajectory recomputation for the final candidates.
    /// Event affinity/repetition are checked by behavioral tests; this verifies
    /// all removable motion contributions and the incremental cross ledger.
    pub fn verify(&self, state: &State) -> Result<(), Diagnostic> {
        let mut expected = ScoreBreakdown::default();
        let left = state.arms[0].segments.to_vec();
        let right = state.arms[1].segments.to_vec();
        for (segments, hand) in [(&left, Hand::L), (&right, Hand::R)] {
            for segment in segments {
                let t = self.engine.motion_terms(segment, hand).map_err(error)?;
                expected.side_exposure += t.side_exposure;
                expected.travel_strain += t.travel_strain;
                expected.free_distance += t.free_distance;
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
            expected.cross_exposure += self.engine.crossing(&left[i], &right[j]).map_err(error)?;
            let (le, re) = (left[i].end_seconds, right[j].end_seconds);
            if le <= re {
                i += 1;
            }
            if re <= le {
                j += 1;
            }
        }
        let actual = &state.v2.parts;
        for (a, b) in [
            (expected.side_exposure, actual.side_exposure),
            (expected.cross_exposure, actual.cross_exposure),
            (expected.travel_strain, actual.travel_strain),
            (expected.free_distance, actual.free_distance),
            (expected.compression_strain, actual.compression_strain),
        ] {
            if (a - b).abs() > 1e-7 * (1.0 + a.abs()) {
                return Err(error(format!("V2 增量成本不一致：{a} / {b}")));
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
            s.v2.contacts
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

pub(super) fn compare(a: &State, b: &State, v2: bool, final_rank: bool) -> std::cmp::Ordering {
    if !v2 {
        return if final_rank {
            a.cost.total().total_cmp(&b.cost.total())
        } else {
            a.rank().total_cmp(&b.rank())
        };
    }
    let left = if final_rank {
        &a.v2.score
    } else {
        &a.v2.rank_score
    };
    let right = if final_rank {
        &b.v2.score
    } else {
        &b.v2.rank_score
    };
    left.as_ref()
        .unwrap()
        .compare(right.as_ref().unwrap())
        .then_with(|| a.deposit.len().cmp(&b.deposit.len()))
}
