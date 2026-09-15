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
/// 兩條 Slide 要多近才算「碰頭」。必須近到可以視為同一點，兩手才能原地互換目的地
/// 而不產生軌跡跳動；只是空間交叉、時間錯開的不算。
const MEET_TOLERANCE: f64 = 1e-6;
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
    fn last(&self) -> Option<&T> {
        self.head.as_ref().map(|link| &link.value)
    }
    /// 取下最後一項，用來把已經記錄的停留改寫成滑移。
    fn pop(&self) -> Option<(Self, T)> {
        let link = self.head.as_ref()?;
        Some((link.prev.clone(), link.value.clone()))
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
    /// 接觸任務的固定位置取樣；Slide 的取樣依實際接上的時間現算，這裡為空。
    samples: Vec<MotionSample>,
    /// chart.paths 的索引，只有 Slide 任務有。
    path: usize,
    /// 這條 Slide 的最後一段：到這裡還沒接上就無法再延後。
    last: bool,
    /// 整條路徑的弧長與名目移動時間，用來估延後接上時尚未付出的移動量。
    path_length: f64,
    span: f64,
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
    /// 每條進行中的 Slide 實際被接上的時間；手晚接上時，剩下的路徑就壓縮在剩餘時間內走完。
    engaged: BTreeMap<usize, f64>,
    /// 尚未接上的 Slide 已累積多少「遲早要付」的移動量。延後接上本身不花成本，
    /// 若直接比較累計成本，延後的狀態永遠比準時的便宜，beam 會把準時解全部剪掉。
    /// 這筆金額只加進剪枝用的排序鍵，不進入回報的成本，接上時歸零。
    pending: f64,
    deposit: BTreeMap<usize, f64>,
    last_handover: BTreeMap<usize, f64>,
}
impl State {
    /// Beam 剪枝用的排序鍵：已付出的成本加上尚未付出的移動量。
    fn rank(&self) -> f64 {
        self.cost.total() + self.pending
    }
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

/// 依照準時追蹤的排程，某條 Slide 在 time 當下的位置。
fn slide_point(chart: &Chart, note: &Note, time: f64) -> Point {
    let path = chart
        .paths
        .iter()
        .find(|p| Some(&p.id) == note.path_id.as_ref())
        .unwrap();
    let (start, end) = (note.motion_start.unwrap(), note.motion_end.unwrap());
    path.at((time - start) / (end - start))
}

/// 兩條 Slide「碰頭」的時刻：同一瞬間經過同一點。只有真的重合才算，
/// 空間上交叉但時間錯開不算 —— 那種情況手臂本來就得交叉。
fn meetings(chart: &Chart, c: &SolverConfig) -> Vec<(usize, usize, f64)> {
    let slides: Vec<usize> = (0..chart.notes.len())
        .filter(|i| chart.notes[*i].path_id.is_some())
        .collect();
    let mut out = vec![];
    for (k, &i) in slides.iter().enumerate() {
        for &j in &slides[k + 1..] {
            let (a, b) = (&chart.notes[i], &chart.notes[j]);
            let lo = a.motion_start.unwrap().max(b.motion_start.unwrap());
            let hi = a.motion_end.unwrap().min(b.motion_end.unwrap());
            if hi <= lo + EPS {
                continue;
            }
            let gap = |t: f64| slide_point(chart, a, t).distance(slide_point(chart, b, t));
            let steps = (((hi - lo) / (c.checkpoint_seconds / 4.0)).ceil() as usize).clamp(8, 4096);
            let at = |k: usize| lo + (hi - lo) * k as f64 / steps as f64;
            for k in 1..steps {
                let (before, here, after) = (gap(at(k - 1)), gap(at(k)), gap(at(k + 1)));
                if here > before || here > after {
                    continue;
                }
                // 局部極小值：用三分搜尋逼近真正的碰頭時刻。
                let (mut left, mut right) = (at(k - 1), at(k + 1));
                for _ in 0..60 {
                    let m1 = left + (right - left) / 3.0;
                    let m2 = right - (right - left) / 3.0;
                    if gap(m1) <= gap(m2) {
                        right = m2;
                    } else {
                        left = m1;
                    }
                }
                let t = (left + right) / 2.0;
                if gap(t) <= MEET_TOLERANCE {
                    out.push((i, j, t));
                }
            }
        }
    }
    out
}

fn tasks(chart: &Chart, c: &SolverConfig) -> Result<Vec<Task>, Diagnostic> {
    // 先算出每條 Slide 的 checkpoint 時間，再把兩條 Slide 的碰頭時刻插進去，
    // 讓 beam 在那一刻剛好有機會讓兩手互換目的地。
    let mut schedule: Vec<Vec<f64>> = vec![vec![]; chart.notes.len()];
    for (i, n) in chart.notes.iter().enumerate() {
        let (Some(start), Some(end)) = (n.motion_start, n.motion_end) else {
            continue;
        };
        let count = ((end - start) / c.checkpoint_seconds).ceil() as usize;
        if count > MAX_TASKS {
            return Err(Diagnostic::plain(
                "search_limit",
                "單條 Slide 的 checkpoints 超出計算預算，請調大搜尋取樣間隔".into(),
            ));
        }
        let mut times = (0..=count)
            .map(|k| start + (end - start) * k as f64 / count as f64)
            .collect::<Vec<_>>();
        // Include note onsets so a hand can be released in time for a simultaneous event.
        times.extend(
            chart
                .notes
                .iter()
                .map(|n| n.time_seconds)
                .filter(|t| *t > start && *t < end),
        );
        schedule[i] = times;
    }
    if c.allow_handover {
        for (i, j, t) in meetings(chart, c) {
            schedule[i].push(t);
            schedule[j].push(t);
        }
    }
    for times in schedule.iter_mut() {
        times.sort_by(f64::total_cmp);
        times.dedup_by(|a, b| (*a - *b).abs() < EPS);
    }

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
                path: usize::MAX,
                last: false,
                path_length: 0.0,
                span: 0.0,
                continuation: false,
            });
        }
        if let Some(path_id) = &n.path_id {
            let path_index = chart.paths.iter().position(|p| &p.id == path_id).unwrap();
            let start = n.motion_start.unwrap();
            let end = n.motion_end.unwrap();
            let times = &schedule[i];
            let samples = &chart.paths[path_index].samples;
            let length: f64 = samples
                .windows(2)
                .map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y))
                .sum();
            let steps = times.len() - 1;
            for (k, pair) in times.windows(2).enumerate() {
                tasks.push(Task {
                    note: i,
                    start: pair[0],
                    end: pair[1],
                    mode: "slide",
                    samples: vec![],
                    path: path_index,
                    last: k + 1 == steps,
                    path_length: length,
                    span: end - start,
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

/// 一段動作的距離、速度與跨側成本（已乘權重）。抽出來是為了在改寫動作段時
/// 可以先扣掉舊的貢獻，不重複計算。
fn segment_terms(samples: &[MotionSample], hand: Hand, c: &SolverConfig) -> (f64, f64, f64) {
    let (mut distance, mut speed, mut side) = (0.0, 0.0, 0.0);
    for pair in samples.windows(2) {
        let dt = pair[1].time_seconds - pair[0].time_seconds;
        let a = pair[0].point();
        let b = pair[1].point();
        let d = a.distance(b);
        distance += c.distance_weight * d;
        if dt > EPS {
            speed += c.speed_weight * d * d / dt / c.speed_reference.powi(2);
            // Midpoint quadrature is stable under path subdivision, including clipping at x=0.
            let sign = if hand == Hand::L { 1.0 } else { -1.0 };
            for i in 0..8 {
                let x = a.x + (b.x - a.x) * (i as f64 + 0.5) / 8.0;
                side += c.side_weight * (sign * x).max(0.0).powi(2) * dt / 8.0;
            }
        }
    }
    (distance, speed, side)
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
    let (distance, speed, side) = segment_terms(&samples, hand, c);
    cost.distance += distance;
    cost.speed += speed;
    cost.side += side;
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

/// 兩手在同一點碰頭時，原地互換各自要追的 Slide。兩手位置相同，交換之後軌跡仍然連續，
/// 因此不需要任何移動 —— 這正是玩家在對穿的 Slide 上避免手臂交叉的作法。
fn swap_at(state: &State, time: f64, chart: &Chart, c: &SolverConfig) -> Option<State> {
    if !c.allow_handover {
        return None;
    }
    let [left, right] = &state.arms;
    if (left.free - time).abs() > EPS
        || (right.free - time).abs() > EPS
        || left.point.distance(right.point) > MEET_TOLERANCE
    {
        return None;
    }
    // 只處理「一手一條」的單純情形，避免三條以上同時進行時語意不明。
    if state.owners.len() != 2 {
        return None;
    }
    let mut pairs = state.owners.iter();
    let (&first, &first_hand) = pairs.next()?;
    let (&second, &second_hand) = pairs.next()?;
    if first_hand == second_hand {
        return None;
    }
    for note in [first, second] {
        let n = &chart.notes[note];
        if time >= n.motion_end? - EPS {
            return None;
        }
        if state
            .last_handover
            .get(&note)
            .is_some_and(|t| time - t < c.handover_cooldown - EPS)
        {
            return None;
        }
    }
    let mut next = state.clone();
    next.owners.insert(first, second_hand);
    next.owners.insert(second, first_hand);
    next.last_handover.insert(first, time);
    next.last_handover.insert(second, time);
    // 兩手位置相同，互換不需要任何移動，也沒有交接重疊；因此不收交接費用。
    // 會不會互換，交由交叉與跨側成本決定。
    for (note, from, to) in [
        (first, first_hand, second_hand),
        (second, second_hand, first_hand),
    ] {
        next.handovers = next.handovers.push(Handover {
            note_id: chart.notes[note].id.clone(),
            from,
            to,
            start_seconds: time,
            end_seconds: time,
            swap: true,
        });
    }
    Some(next)
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

    // Slide 的移動起點只是星星出發的時刻，不是手一定要貼上去的時刻。
    // 手可以晚一點才接上，剩下的路徑就壓縮在剩餘時間內走完；走得越急，速度成本越高。
    let engaged = state.engaged.get(&task.note).copied();
    let (begin, pickup) = if task.mode == "slide" {
        let finish = n.motion_end.unwrap();
        let pickup = engaged.unwrap_or_else(|| task.start.max(state.arms[idx].free));
        let begin = task.start.max(pickup);
        if begin >= task.end - EPS || pickup >= finish - EPS {
            return None;
        }
        if engaged.is_none() && pickup > n.motion_start.unwrap() + c.slide_pickup_seconds + EPS {
            return None;
        }
        (begin, Some(pickup))
    } else {
        (task.start, None)
    };
    let samples = match pickup {
        Some(pickup) => path_samples(
            &chart.paths[task.path],
            pickup,
            n.motion_end.unwrap(),
            begin,
            task.end,
        ),
        None => task.samples.clone(),
    };
    let start = samples[0].point();

    if state.arms[idx].free > begin + EPS {
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
    if begin - state.arms[idx].free <= EPS && state.arms[idx].point.distance(start) > EPS {
        return None;
    }
    let old = state.owners.get(&task.note).copied();
    let switching = engaged.is_some() && old != Some(hand);
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

    // 相鄰又接得很緊的連續接觸不必抬手：真實打法是手貼著面板等速滑過去，而不是
    // 「停 contactSeconds 再衝刺」。符合條件時把前一次接觸的停留改寫成滑移的前半段，
    // 整段 [前一顆判定時間, 這一顆判定時間] 因此是等速移動，速度負擔照實際情形計算。
    // 間隔拉開到 repetitionSeconds 以上就有餘裕抬手，一般人也會抬手，因此不套用。
    let glide = matches!(task.mode, "tap" | "hold")
        && c.glide_distance > 0.0
        && next.arms[idx].free <= begin + EPS
        && next.arms[idx].segments.last().is_some_and(|s| {
            s.mode == "tap"
                && begin > s.start_seconds + EPS
                && begin - s.start_seconds < c.repetition_seconds - EPS
        })
        && {
            let d = next.arms[idx].point.distance(start);
            d > EPS && d <= c.glide_distance
        };
    if glide {
        let arm = &mut next.arms[idx];
        let (rest, previous) = arm.segments.pop().unwrap();
        let (distance, speed, side) = segment_terms(&previous.samples, hand, c);
        next.cost.distance -= distance;
        next.cost.speed -= speed;
        next.cost.side -= side;
        let from = previous.samples[0].point();
        let u = (previous.end_seconds - previous.start_seconds) / (begin - previous.start_seconds);
        let samples = vec![
            MotionSample::new(previous.start_seconds, from),
            MotionSample::new(previous.end_seconds, from.lerp(start, u)),
        ];
        let (distance, speed, side) = segment_terms(&samples, hand, c);
        next.cost.distance += distance;
        next.cost.speed += speed;
        next.cost.side += side;
        arm.point = samples[1].point();
        arm.segments = rest.push(MotionSegment {
            mode: "glide".into(),
            note_id: previous.note_id,
            start_seconds: previous.start_seconds,
            end_seconds: previous.end_seconds,
            samples,
        });
    }

    let a = &mut next.arms[idx];
    if begin > a.free + EPS {
        let samples = vec![
            MotionSample::new(a.free, a.point),
            MotionSample::new(begin, start),
        ];
        let mode = if glide {
            "glide"
        } else if a.point.distance(start) < EPS {
            "idle"
        } else {
            "travel"
        };
        add_segment(a, hand, samples, mode, None, &mut next.cost, c);
    }
    if task.mode == "tap" {
        // 滑移代表手沒有離開面板，不算一次重新擊打。
        if let (Some(t), false) = (a.last_tap, glide) {
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
        samples,
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
        start_seconds: begin,
        end_seconds: task.end,
    });
    if let Some(pickup) = pickup {
        next.owners.insert(task.note, hand);
        if next.engaged.insert(task.note, pickup).is_none() {
            next.pending -= next.deposit.remove(&task.note).unwrap_or(0.0);
        }
    }
    if switching {
        let old = old.unwrap();
        let until = task.start + c.handover_seconds;
        let samples = path_samples(
            &chart.paths[task.path],
            pickup.unwrap(),
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
            swap: false,
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
        engaged: BTreeMap::new(),
        pending: 0.0,
        deposit: BTreeMap::new(),
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
            state.engaged.retain(|note, _| live(note));
            state.last_handover.retain(|note, _| live(note));
            state.deposit.retain(|note, held| {
                let keep = live(note);
                if !keep {
                    state.pending -= *held;
                }
                keep
            });
        }
        // 還沒接上的 Slide 可以先不接，讓手去處理起點與移動之間安排的其他音符；
        // 最後一段不能再延後，否則這條 Slide 就沒人走了。
        // 超過最晚接上時間之後就不必再產生「先不接」的分支，那些狀態走不下去。
        let deferrable = task.mode == "slide"
            && !task.last
            && task.end
                <= chart.notes[task.note].motion_start.unwrap() + c.slide_pickup_seconds + EPS;
        // 兩手若在這一刻碰頭，先把「互換目的地」的變化加進 beam，再一起展開這個任務。
        let swapped: Vec<State> = beam
            .iter()
            .filter_map(|state| swap_at(state, task.start, chart, c))
            .collect();
        beam.extend(swapped);

        // 延後接上這條 Slide，遲早還是要走完整條路徑：先把那段移動與速度負擔計入排序，
        // 否則「先不接」永遠比已經付出成本的分支便宜，beam 會把準時的解全部剪掉。
        let share = if deferrable {
            let dt = task.end - task.start;
            let span = task.span.max(EPS);
            c.distance_weight * task.path_length * dt / span
                + c.speed_weight * task.path_length.powi(2) * dt
                    / span.powi(2)
                    / c.speed_reference.powi(2)
        } else {
            0.0
        };
        let mut next = vec![];
        for state in &beam {
            for hand in [Hand::L, Hand::R] {
                expansions += 1;
                if let Some(s) = assign(state, task, hand, chart, c) {
                    next.push(s);
                }
            }
            if deferrable && !state.engaged.contains_key(&task.note) {
                let mut deferred = state.clone();
                deferred.pending += share;
                *deferred.deposit.entry(task.note).or_default() += share;
                next.push(deferred);
            }
        }
        if next.is_empty() {
            let mut d=Diagnostic::plain("no_solution","本模型未找到可行方案：手被佔用或無法連續接觸。這不代表人類無法遊玩；可增加 beamWidth 或縮短片段。".into());
            d.time_seconds = Some(task.start);
            d.note_ids = vec![chart.notes[task.note].id.clone()];
            d.source_span = Some(Box::new(chart.notes[task.note].source_span.clone()));
            return Err(d);
        }
        next.sort_by(|a, b| a.rank().total_cmp(&b.rank()));
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
