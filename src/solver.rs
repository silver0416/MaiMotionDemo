use crate::geometry::button;
use crate::*;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

const EPS: f64 = 1e-8;

/// 計算預算。狀態複製已是常數成本，上限因此由整體工作量與時間決定，
/// 不再靠音符數量的硬性上限來遮掩搜尋成本。
/// MAX_TASKS 同時是記憶體防線：每個事件在 beam 中約佔 26 KB，
/// 實測一首完整譜面（857 個音符、約 1100 個事件）峰值只有 19 MB。
const MAX_TASKS: usize = 50_000;
const MAX_EXPANSIONS: u64 = 120_000_000;
const BUDGET: Duration = Duration::from_secs(20);

/// 不可變的單向串列。Beam Search 每展開一步就要複製一份狀態；用結構共享把
/// 複製成本壓到 O(1)，整體搜尋才會隨音符數線性成長，而不是平方成長。
struct Chain<T> {
    head: Option<Arc<Link<T>>>,
    len: usize,
}
struct Link<T> {
    prev: Chain<T>,
    value: T,
}
impl<T> Clone for Chain<T> {
    fn clone(&self) -> Self {
        Self {
            head: self.head.clone(),
            len: self.len,
        }
    }
}
impl<T> Default for Chain<T> {
    fn default() -> Self {
        Self { head: None, len: 0 }
    }
}
impl<T: Clone> Chain<T> {
    fn push(&self, value: T) -> Self {
        Self {
            head: Some(Arc::new(Link {
                prev: self.clone(),
                value,
            })),
            len: self.len + 1,
        }
    }
    fn to_vec(&self) -> Vec<T> {
        let mut out = Vec::with_capacity(self.len);
        let mut cursor = self.head.as_ref();
        while let Some(link) = cursor {
            out.push(link.value.clone());
            cursor = link.prev.head.as_ref();
        }
        out.reverse();
        out
    }
}
/// 逐節釋放，長譜面的串列才不會在遞迴 drop 時爆堆疊。
impl<T> Drop for Chain<T> {
    fn drop(&mut self) {
        let mut cursor = self.head.take();
        while let Some(link) = cursor {
            match Arc::try_unwrap(link) {
                Ok(mut link) => cursor = link.prev.head.take(),
                Err(_) => break,
            }
        }
    }
}

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
    segments: Chain<MotionSegment>,
}
#[derive(Clone)]
struct State {
    arms: [Arm; 2],
    cost: CostBreakdown,
    assignments: Chain<Assignment>,
    handovers: Chain<Handover>,
    /// 只保留還在進行中的 Slide；結束的項目每個時間點清掉，複製成本才不會隨譜面長度成長。
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
        let holding = n.kind == "hold" || n.kind == "touchHold";
        let end = if holding {
            n.end_seconds
        } else {
            n.time_seconds + c.contact_seconds
        };
        // `?` `!` 與 `*` 的第二條之後沒有起點觸碰，不建立接觸任務。
        if n.has_head {
            tasks.push(Task {
                note: i,
                start: n.time_seconds,
                end,
                mode: if holding { "hold" } else { "tap" },
                samples: vec![
                    MotionSample::new(n.time_seconds, n.position),
                    MotionSample::new(end, n.position),
                ],
                continuation: false,
            });
        }
        if let Some(path_id) = &n.path_id {
            let path = chart.paths.iter().find(|p| &p.id == path_id).unwrap();
            let start = n.motion_start.unwrap();
            let end = n.motion_end.unwrap();
            let count = ((end - start) / c.checkpoint_seconds).ceil() as usize;
            if count > MAX_TASKS {
                return Err(Diagnostic::plain(
                    "search_limit",
                    "單條 Slide 的 checkpoints 超出計算預算，請調大搜尋取樣間隔".into(),
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
    if tasks.len() > MAX_TASKS {
        return Err(Diagnostic::plain(
            "search_limit",
            "事件數超出計算預算，請縮短片段或調大搜尋取樣間隔".into(),
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
    arm.segments = arm.segments.push(MotionSegment {
        mode: mode.into(),
        note_id,
        start_seconds: first.time_seconds,
        end_seconds: last.time_seconds,
        samples,
    });
}

/// 手在 time 當下的位置。time 落在目前這段動作之內時要插值，不能只看段尾。
fn arm_point_at(arm: &Arm, time: f64) -> Point {
    let Some(link) = arm.segments.head.as_ref() else {
        return arm.point;
    };
    let samples = &link.value.samples;
    if time <= samples[0].time_seconds {
        return samples[0].point();
    }
    let index = samples
        .partition_point(|s| s.time_seconds < time)
        .clamp(1, samples.len() - 1);
    let (a, b) = (&samples[index - 1], &samples[index]);
    let dt = b.time_seconds - a.time_seconds;
    if dt <= EPS {
        return b.point();
    }
    a.point()
        .lerp(b.point(), ((time - a.time_seconds) / dt).clamp(0.0, 1.0))
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
        // 手上已有任務，但接觸點就在同一個位置：同一隻手一次接觸即可滿足兩者
        // （例如 Slide 起點上的 Break Tap）。不另外產生軌跡段，也不另外計成本。
        if task.mode == "tap" && arm_point_at(&state.arms[idx], task.start).distance(start) <= EPS {
            let mut next = state.clone();
            next.arms[idx].last_tap = Some(task.start);
            next.assignments = next.assignments.push(Assignment {
                note_id: n.id.clone(),
                part: if n.kind == "slide" { "head" } else { "contact" }.into(),
                hand,
                start_seconds: task.start,
                end_seconds: task.end,
            });
            return Some(next);
        }
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
    next.assignments = next.assignments.push(Assignment {
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
        next.assignments = next.assignments.push(Assignment {
            note_id: n.id.clone(),
            part: "slide".into(),
            hand: old,
            start_seconds: task.start,
            end_seconds: until,
        });
        next.handovers = next.handovers.push(Handover {
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

/// 把連續的動作段攤平成單調的取樣序列，供交叉成本以線性掃描計算。
fn flatten(segments: &[MotionSegment]) -> Vec<MotionSample> {
    let mut out: Vec<MotionSample> = vec![];
    for segment in segments {
        for sample in &segment.samples {
            if out
                .last()
                .is_some_and(|last| (last.time_seconds - sample.time_seconds).abs() < EPS)
            {
                continue;
            }
            out.push(sample.clone());
        }
    }
    out
}

/// 查詢時間單調遞增，游標只會前進，因此整段掃描是線性的。
fn advance(samples: &[MotionSample], cursor: &mut usize, time: f64) -> Point {
    while *cursor + 1 < samples.len() && samples[*cursor + 1].time_seconds < time {
        *cursor += 1;
    }
    let a = &samples[*cursor];
    let b = &samples[(*cursor + 1).min(samples.len() - 1)];
    let dt = b.time_seconds - a.time_seconds;
    if dt <= EPS {
        return b.point();
    }
    a.point()
        .lerp(b.point(), ((time - a.time_seconds) / dt).clamp(0.0, 1.0))
}

fn cross_cost(left: &[MotionSegment], right: &[MotionSegment], c: &SolverConfig) -> f64 {
    let (left, right) = (flatten(left), flatten(right));
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    // 兩手取樣時間各自遞增，直接合併成共同的積分格點。
    let mut times = Vec::with_capacity(left.len() + right.len());
    let (mut i, mut j) = (0, 0);
    while i < left.len() || j < right.len() {
        let t = match (left.get(i), right.get(j)) {
            (Some(a), Some(b)) => {
                if a.time_seconds <= b.time_seconds {
                    i += 1;
                    a.time_seconds
                } else {
                    j += 1;
                    b.time_seconds
                }
            }
            (Some(a), None) => {
                i += 1;
                a.time_seconds
            }
            (None, Some(b)) => {
                j += 1;
                b.time_seconds
            }
            (None, None) => break,
        };
        if times
            .last()
            .is_none_or(|last: &f64| (t - *last).abs() >= EPS)
        {
            times.push(t);
        }
    }
    let mut cost = 0.0;
    let (mut lc, mut rc) = (0, 0);
    for pair in times.windows(2) {
        for k in 0..4 {
            let t = pair[0] + (pair[1] - pair[0]) * (k as f64 + 0.5) / 4.0;
            let x = advance(&left, &mut lc, t).x - advance(&right, &mut rc, t).x;
            cost += x.max(0.0).powi(2) * (pair[1] - pair[0]) / 4.0;
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
            segments: Chain::default(),
        },
        Arm {
            point: button(2),
            free: start,
            last_tap: None,
            segments: Chain::default(),
        },
    ];
    let mut beam = vec![State {
        arms,
        cost: CostBreakdown::default(),
        assignments: Chain::default(),
        handovers: Chain::default(),
        owners: BTreeMap::new(),
        last_handover: BTreeMap::new(),
    }];
    let tasks = tasks(chart, c)?;
    let clock = Instant::now();
    let mut expansions: u64 = 0;
    for task in &tasks {
        // 已經結束的 Slide 不會再被查詢；清掉之後每個狀態要複製的資料量才是常數。
        for state in &mut beam {
            let live = |note: &usize| {
                chart.notes[*note].motion_end.unwrap_or(f64::INFINITY) >= task.start - EPS
            };
            state.owners.retain(|note, _| live(note));
            state.last_handover.retain(|note, _| live(note));
        }
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
        if expansions > MAX_EXPANSIONS || clock.elapsed() > BUDGET {
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
        let left = state.arms[0].segments.to_vec();
        let right = state.arms[1].segments.to_vec();
        state.cost.cross = cross_cost(&left, &right, c);
    }
    beam.sort_by(|a, b| a.cost.total().total_cmp(&b.cost.total()));
    let mut solutions = vec![];
    let mut signatures = std::collections::BTreeSet::new();
    for state in beam {
        let assignments = merge_assignments(state.assignments.to_vec());
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
            handovers: state.handovers.to_vec(),
            left_segments: left.segments.to_vec(),
            right_segments: right.segments.to_vec(),
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
