use crate::geometry::button;
use crate::*;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

const EPS: f64 = 1e-8;

#[derive(Clone)]
struct Task {
    note: usize,
    start: f64,
    end: f64,
    mode: &'static str,
    samples: Vec<MotionSample>,
    continuation: bool,
}
#[derive(Clone)]
struct Arm {
    point: Point,
    free: f64,
    last_tap: Option<f64>,
    segments: Vec<MotionSegment>,
}
#[derive(Clone)]
struct State {
    arms: [Arm; 2],
    cost: CostBreakdown,
    assignments: Vec<Assignment>,
    handovers: Vec<Handover>,
    owners: BTreeMap<usize, Hand>,
    last_handover: BTreeMap<usize, f64>,
}

fn path_samples(
    path: &SlidePath,
    motion_start: f64,
    motion_end: f64,
    start: f64,
    end: f64,
) -> Vec<MotionSample> {
    let u0 = (start - motion_start) / (motion_end - motion_start);
    let u1 = (end - motion_start) / (motion_end - motion_start);
    let mut samples = vec![MotionSample::new(start, path.at(u0))];
    for p in &path.samples {
        if p.u > u0 + EPS && p.u < u1 - EPS {
            samples.push(MotionSample::new(
                motion_start + p.u * (motion_end - motion_start),
                Point { x: p.x, y: p.y },
            ));
        }
    }
    samples.push(MotionSample::new(end, path.at(u1)));
    samples
}

fn tasks(chart: &Chart, c: &SolverConfig) -> Result<Vec<Task>, Diagnostic> {
    let mut tasks = vec![];
    for (i, n) in chart.notes.iter().enumerate() {
        let end = if n.kind == "hold" {
            n.end_seconds
        } else {
            n.time_seconds + c.contact_seconds
        };
        tasks.push(Task {
            note: i,
            start: n.time_seconds,
            end,
            mode: if n.kind == "hold" { "hold" } else { "tap" },
            samples: vec![
                MotionSample::new(n.time_seconds, n.position),
                MotionSample::new(end, n.position),
            ],
            continuation: false,
        });
        if let Some(path_id) = &n.path_id {
            let path = chart.paths.iter().find(|p| &p.id == path_id).unwrap();
            let start = n.motion_start.unwrap();
            let end = n.motion_end.unwrap();
            let count = ((end - start) / c.checkpoint_seconds).ceil() as usize;
            if count > 12000 {
                return Err(Diagnostic::plain(
                    "search_limit",
                    "Slide checkpoints 超出 Demo 計算預算".into(),
                ));
            }
            // Include note onsets so a hand can be released in time for a simultaneous event.
            let mut times = (0..=count)
                .map(|k| start + (end - start) * k as f64 / count as f64)
                .collect::<Vec<_>>();
            times.extend(
                chart
                    .notes
                    .iter()
                    .map(|n| n.time_seconds)
                    .filter(|t| *t > start && *t < end),
            );
            times.sort_by(f64::total_cmp);
            times.dedup_by(|a, b| (*a - *b).abs() < EPS);
            for (k, pair) in times.windows(2).enumerate() {
                tasks.push(Task {
                    note: i,
                    start: pair[0],
                    end: pair[1],
                    mode: "slide",
                    samples: path_samples(path, start, end, pair[0], pair[1]),
                    continuation: k > 0,
                });
            }
        }
    }
    tasks.sort_by(|a, b| {
        a.start
            .total_cmp(&b.start)
            .then_with(|| b.continuation.cmp(&a.continuation))
            .then_with(|| chart.notes[a.note].button.cmp(&chart.notes[b.note].button))
            .then_with(|| a.mode.cmp(b.mode))
            .then_with(|| a.note.cmp(&b.note))
    });
    if tasks.len() > 12000 {
        return Err(Diagnostic::plain(
            "search_limit",
            "事件數超出 Demo 計算預算，請縮短片段".into(),
        ));
    }
    Ok(tasks)
}

fn add_segment(
    arm: &mut Arm,
    hand: Hand,
    samples: Vec<MotionSample>,
    mode: &str,
    note_id: Option<String>,
    cost: &mut CostBreakdown,
    c: &SolverConfig,
) {
    for pair in samples.windows(2) {
        let dt = pair[1].time_seconds - pair[0].time_seconds;
        let a = pair[0].point();
        let b = pair[1].point();
        let d = a.distance(b);
        cost.distance += c.distance_weight * d;
        if dt > EPS {
            cost.speed += c.speed_weight * d * d / dt / c.speed_reference.powi(2);
            // Midpoint quadrature is stable under path subdivision, including clipping at x=0.
            let sign = if hand == Hand::L { 1.0 } else { -1.0 };
            for i in 0..8 {
                let x = a.x + (b.x - a.x) * (i as f64 + 0.5) / 8.0;
                cost.side += c.side_weight * (sign * x).max(0.0).powi(2) * dt / 8.0;
            }
        }
    }
    let first = samples.first().unwrap();
    let last = samples.last().unwrap();
    arm.point = last.point();
    arm.free = last.time_seconds;
    arm.segments.push(MotionSegment {
        mode: mode.into(),
        note_id,
        start_seconds: first.time_seconds,
        end_seconds: last.time_seconds,
        samples,
    });
}

fn assign(
    state: &State,
    task: &Task,
    hand: Hand,
    chart: &Chart,
    c: &SolverConfig,
) -> Option<State> {
    let idx = hand.index();
    let n = &chart.notes[task.note];
    let start = task.samples[0].point();
    if state.arms[idx].free > task.start + EPS {
        return None;
    }
    if task.start - state.arms[idx].free <= EPS && state.arms[idx].point.distance(start) > EPS {
        return None;
    }
    let old = state.owners.get(&task.note).copied();
    let switching = task.continuation && old != Some(hand);
    if switching {
        if !c.allow_handover || task.end - task.start + EPS < c.handover_seconds {
            return None;
        }
        if state
            .last_handover
            .get(&task.note)
            .is_some_and(|t| task.start - t < c.handover_cooldown - EPS)
        {
            return None;
        }
        let old_idx = old?.index();
        if (state.arms[old_idx].free - task.start).abs() > EPS
            || state.arms[old_idx].point.distance(start) > EPS
        {
            return None;
        }
    }
    let mut next = state.clone();
    let a = &mut next.arms[idx];
    if task.start > a.free + EPS {
        let samples = vec![
            MotionSample::new(a.free, a.point),
            MotionSample::new(task.start, start),
        ];
        let mode = if a.point.distance(start) < EPS {
            "idle"
        } else {
            "travel"
        };
        add_segment(a, hand, samples, mode, None, &mut next.cost, c);
    }
    if task.mode == "tap" {
        if let Some(t) = a.last_tap {
            next.cost.repetition += c.repetition_weight
                * (1.0 - (task.start - t) / c.repetition_seconds)
                    .max(0.0)
                    .powi(2);
        }
        a.last_tap = Some(task.start);
    }
    add_segment(
        a,
        hand,
        task.samples.clone(),
        task.mode,
        Some(n.id.clone()),
        &mut next.cost,
        c,
    );
    next.assignments.push(Assignment {
        note_id: n.id.clone(),
        part: if task.mode == "slide" {
            "slide"
        } else if n.kind == "slide" {
            "head"
        } else {
            "contact"
        }
        .into(),
        hand,
        start_seconds: task.start,
        end_seconds: task.end,
    });
    if task.mode == "slide" {
        next.owners.insert(task.note, hand);
    }
    if switching {
        let old = old.unwrap();
        let until = task.start + c.handover_seconds;
        let path = chart
            .paths
            .iter()
            .find(|p| Some(&p.id) == n.path_id.as_ref())
            .unwrap();
        let samples = path_samples(
            path,
            n.motion_start.unwrap(),
            n.motion_end.unwrap(),
            task.start,
            until,
        );
        add_segment(
            &mut next.arms[old.index()],
            old,
            samples,
            "handover",
            Some(n.id.clone()),
            &mut next.cost,
            c,
        );
        next.assignments.push(Assignment {
            note_id: n.id.clone(),
            part: "slide".into(),
            hand: old,
            start_seconds: task.start,
            end_seconds: until,
        });
        next.handovers.push(Handover {
            note_id: n.id.clone(),
            from: old,
            to: hand,
            start_seconds: task.start,
            end_seconds: until,
        });
        next.last_handover.insert(task.note, task.start);
        next.cost.handover += c.handover_weight;
    }
    Some(next)
}

fn position(segments: &[MotionSegment], time: f64) -> Point {
    let i = segments
        .partition_point(|s| s.end_seconds < time)
        .min(segments.len() - 1);
    let samples = &segments[i].samples;
    let j = samples
        .partition_point(|s| s.time_seconds < time)
        .clamp(1, samples.len() - 1);
    let a = &samples[j - 1];
    let b = &samples[j];
    let dt = b.time_seconds - a.time_seconds;
    a.point().lerp(
        b.point(),
        if dt <= EPS {
            1.0
        } else {
            ((time - a.time_seconds) / dt).clamp(0.0, 1.0)
        },
    )
}

fn cross_cost(arms: &[Arm; 2], c: &SolverConfig) -> f64 {
    let mut times = arms
        .iter()
        .flat_map(|a| {
            a.segments
                .iter()
                .flat_map(|s| s.samples.iter().map(|p| p.time_seconds))
        })
        .collect::<Vec<_>>();
    times.sort_by(f64::total_cmp);
    times.dedup_by(|a, b| (*a - *b).abs() < EPS);
    let mut cost = 0.0;
    for p in times.windows(2) {
        for i in 0..4 {
            let t = p[0] + (p[1] - p[0]) * (i as f64 + 0.5) / 4.0;
            let x = position(&arms[0].segments, t).x - position(&arms[1].segments, t).x;
            cost += x.max(0.0).powi(2) * (p[1] - p[0]) / 4.0;
        }
    }
    c.cross_weight * cost
}

fn merge_assignments(mut assignments: Vec<Assignment>) -> Vec<Assignment> {
    assignments.sort_by(|a, b| {
        a.note_id
            .cmp(&b.note_id)
            .then(a.part.cmp(&b.part))
            .then(a.hand.index().cmp(&b.hand.index()))
            .then(a.start_seconds.total_cmp(&b.start_seconds))
    });
    let mut out: Vec<Assignment> = vec![];
    for a in assignments {
        if let Some(last) = out.last_mut() {
            if last.note_id == a.note_id
                && last.part == a.part
                && last.hand == a.hand
                && (last.end_seconds - a.start_seconds).abs() < EPS
            {
                last.end_seconds = a.end_seconds;
                continue;
            }
        }
        out.push(a);
    }
    out
}

pub fn solve(chart: &Chart, c: &SolverConfig) -> Result<Vec<Solution>, Diagnostic> {
    let start = chart
        .notes
        .iter()
        .map(|n| n.time_seconds)
        .fold(f64::INFINITY, f64::min)
        - c.preparation_seconds;
    let arms = [
        Arm {
            point: button(7),
            free: start,
            last_tap: None,
            segments: vec![],
        },
        Arm {
            point: button(2),
            free: start,
            last_tap: None,
            segments: vec![],
        },
    ];
    let mut beam = vec![State {
        arms,
        cost: CostBreakdown::default(),
        assignments: vec![],
        handovers: vec![],
        owners: BTreeMap::new(),
        last_handover: BTreeMap::new(),
    }];
    let tasks = tasks(chart, c)?;
    let clock = Instant::now();
    let mut expansions = 0;
    for task in &tasks {
        let mut next = vec![];
        for state in &beam {
            for hand in [Hand::L, Hand::R] {
                expansions += 1;
                if let Some(s) = assign(state, task, hand, chart, c) {
                    next.push(s);
                }
            }
        }
        if next.is_empty() {
            let mut d=Diagnostic::plain("no_solution","本模型未找到可行方案：手被佔用或無法連續接觸。這不代表人類無法遊玩；可增加 beamWidth 或縮短片段。".into());
            d.time_seconds = Some(task.start);
            d.note_ids = vec![chart.notes[task.note].id.clone()];
            d.source_span = Some(Box::new(chart.notes[task.note].source_span.clone()));
            return Err(d);
        }
        next.sort_by(|a, b| a.cost.total().total_cmp(&b.cost.total()));
        next.truncate(c.beam_width);
        beam = next;
        if expansions > 350_000 || clock.elapsed() > Duration::from_secs(20) {
            return Err(Diagnostic::plain(
                "search_limit",
                "分析已達計算預算，請縮短片段或降低 beamWidth".into(),
            ));
        }
    }
    let end = chart
        .duration_seconds
        .max(tasks.iter().map(|t| t.end).fold(0.0, f64::max));
    for state in &mut beam {
        for hand in [Hand::L, Hand::R] {
            let a = &mut state.arms[hand.index()];
            if a.free < end {
                let samples = vec![
                    MotionSample::new(a.free, a.point),
                    MotionSample::new(end, a.point),
                ];
                add_segment(a, hand, samples, "idle", None, &mut state.cost, c);
            }
        }
        // Crossing is evaluated on both completed trajectories. Beam pruning uses the
        // other additive terms; future free-hand travel is not known until assigned.
        state.cost.cross = cross_cost(&state.arms, c);
    }
    beam.sort_by(|a, b| a.cost.total().total_cmp(&b.cost.total()));
    let mut solutions = vec![];
    let mut signatures = std::collections::BTreeSet::new();
    for state in beam {
        let assignments = merge_assignments(state.assignments);
        let signature = serde_json::to_string(&assignments).unwrap();
        if !signatures.insert(signature) {
            continue;
        }
        let [left, right] = state.arms;
        solutions.push(Solution {
            id: format!("solution-{}", solutions.len() + 1),
            total_cost: state.cost.total(),
            cost_breakdown: state.cost,
            assignments,
            handovers: state.handovers,
            left_segments: left.segments,
            right_segments: right.segments,
            config_snapshot: c.clone(),
            warnings: vec![
                "單手單接觸點的幾何近似，未模擬實機感測器判定或手臂關節。".into(),
                "Beam Search 不保證全域最優；交叉成本於候選完整後排序，搜尋剪枝依其他成本。".into(),
            ],
        });
        if solutions.len() == c.top_k {
            break;
        }
    }
    Ok(solutions)
}
