mod v2;
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
const MAX_GROUP_STATES: usize = 500_000;
const BUDGET: Duration = Duration::from_secs(20);
/// 大量同時 Touch 無法靠兩個固定掌面覆蓋時，允許雙手在判定前貼著面板掃過。
/// 這是 Demo 的可玩性近似；0.18 秒不是官方判定窗。
const TOUCH_SWEEP_SECONDS: f64 = 0.18;
const MIN_TOUCH_SWEEP_NOTES: usize = 16;

// 以下取自玩家整理的 maimai DX 判定規則（60 fps 取樣）。只用來放寬「手何時可以離開
// 上一個接觸」，不模擬判定結果本身；目標仍是每顆音符都落在 Critical Perfect 區間。
/// 判定區與按鍵每幀檢查一次；一次接觸至少要被一個取樣點看到。
const JUDGE_FRAME: f64 = 1.0 / 60.0;
/// Touch 沒有 Fast 判定，正解後 9 幀內接觸仍是 Critical Perfect。
const TOUCH_LATE_FRAMES: f64 = 9.0;
/// Hold 與 Touch Hold 結尾 12 幀不檢查按壓，提早這麼多放手不影響判定。
const HOLD_TAIL_FRAMES: f64 = 12.0;
/// 開頭不檢查按壓的幀數；總長不超過「開頭＋結尾」的短 Hold 只看頭判。
const HOLD_HEAD_FRAMES: f64 = 6.0;
const TOUCH_HOLD_HEAD_FRAMES: f64 = 15.0;
/// 貼著面板移動或按住的手順帶碰到 Touch 的距離。約為相鄰內圈感應區代表點的間距
/// （B5–E6 為 0.16），是 Demo 假設，不是實機感應區面積。
const TOUCH_BRUSH_RADIUS: f64 = 0.2;
/// Slide 尾判 Critical Perfect 的基本半寬與最大半寬（幀）；中央區間向兩側各擴展
/// 「引導星星在最後判定區停留時間 / 4」，另有 −17～−14 幀的 Critical Perfect。
const SLIDE_CRITICAL_FRAMES: f64 = 14.0;
const SLIDE_CRITICAL_MAX_FRAMES: f64 = 36.0;
const SLIDE_CRITICAL_FAST_FRAMES: f64 = 17.0;
/// 按鍵代表點在半徑 1、最外圈感應區在 0.9；介於兩者之間視為螢幕邊界。
const SCREEN_EDGE: f64 = 0.95;

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
    /// 名目終點同時有新接觸時，最後一小段可提前掃完，留下回位時間。
    release_early: bool,
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
    v2: v2::Data,
    group_origin: usize,
    arms: [Arm; 2],
    cost: CostBreakdown,
    assignments: Chain<Assignment>,
    handovers: Chain<Handover>,
    palms: Chain<PalmPlacement>,
    /// 尚在持續、之後可再納入鄰近 Touch 的手掌覆蓋。
    active_palms: [Option<PalmPlacement>; 2],
    /// 以單點開始的 Touch Hold；後續鄰近 Touch 到來時可擴展成手掌。
    held_touch: [Option<usize>; 2],
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
    used_touch_sweep: bool,
    /// 這一組同時音用了判定容許的退路：Touch 晚接觸或 Touch Group 連帶判定。
    /// 同一個 beam 狀態能整組直接完成時，這類分支會被捨棄。
    lenient_in_group: bool,
    /// 手已進入最後判定區而完成的 Slide；其餘 checkpoint 不再需要接觸。
    finished: std::collections::BTreeSet<usize>,
    used_early_slide: bool,
    used_touch_group: bool,
}
struct PalmCandidate {
    covered: Vec<usize>,
    centers: Vec<Point>,
}
/// 手還被上一個接觸佔住時，依判定規則讓它提早空出來後的狀態。
struct Relief {
    state: State,
    /// 新接觸實際開始的時間；Touch 可以比正解晚，其他音符就是正解時間。
    begin: f64,
    /// 上一個接觸已改寫成滑到新目標的 glide，新接觸不算重新擊打。
    glided: bool,
}

/// Hold／Touch Hold 最早可以放手而不影響判定的時間。
fn earliest_release(note: &Note, c: &SolverConfig) -> f64 {
    let head = if note.kind == "touchHold" {
        TOUCH_HOLD_HEAD_FRAMES
    } else {
        HOLD_HEAD_FRAMES
    } * JUDGE_FRAME;
    let tail = HOLD_TAIL_FRAMES * JUDGE_FRAME;
    let duration = note.end_seconds - note.time_seconds;
    if duration <= head + tail + EPS {
        note.time_seconds + c.contact_seconds.min(duration)
    } else {
        note.end_seconds - tail
    }
}

/// 以新的動作段取代手上最後一段，並撤銷舊段的成本。
fn replace_last(
    state: &mut State,
    idx: usize,
    hand: Hand,
    samples: Vec<MotionSample>,
    mode: &str,
    c: &SolverConfig,
) -> Option<()> {
    let arm = &mut state.arms[idx];
    let (rest, previous) = arm.segments.pop()?;
    let (distance, speed, side) = segment_terms(&previous.samples, hand, c);
    state.cost.distance -= distance;
    state.cost.speed -= speed;
    state.cost.side -= side;
    arm.segments = rest;
    add_segment(
        arm,
        hand,
        samples,
        mode,
        previous.note_id,
        &mut state.cost,
        c,
    );
    Some(())
}

/// 手在 time 還被上一組的接觸佔住時，列出依判定規則仍合法的空手方式：
/// 1. 上一個 Tap／手掌只需被一幀看到，可改寫成貼著面板滑向新目標（glide）；
/// 2. Hold／Touch Hold 結尾不檢查按壓，可提早放手；
/// 3. 新目標是 Touch 時，可在 Critical Perfect 區間內晚一點接觸。
///
/// 同一組同時音不適用：它們仍要各自有接觸點，不能靠時間差由同一隻手依序完成。
/// target 為 None 表示 Slide 接軌，只套用第 2 條，接上時間由呼叫端依原規則決定。
/// late=false 只列準時接觸的方式；late=true 只列晚接觸的方式，由呼叫端決定何時採用。
#[allow(clippy::too_many_arguments)]
fn relieve(
    state: &State,
    idx: usize,
    hand: Hand,
    time: f64,
    target: Option<Point>,
    touch: bool,
    late: bool,
    chart: &Chart,
    c: &SolverConfig,
) -> Vec<Relief> {
    let arm = &state.arms[idx];
    let mut out = vec![];
    if arm.free <= time + EPS {
        return out;
    }
    let Some(last) = arm.segments.last().cloned() else {
        return out;
    };
    if last.start_seconds >= time - EPS || last.samples.is_empty() {
        return out;
    }
    let point = last.samples.last().unwrap().point();
    let distance = target.map_or(0.0, |p| point.distance(p));
    let late_limit = time + TOUCH_LATE_FRAMES * JUDGE_FRAME;
    // 非零距離需要至少一幀移動，不能瞬移。
    let arrive = |free: f64| {
        if distance > EPS {
            free + JUDGE_FRAME
        } else {
            free
        }
    };

    if let (Some(target), false) = (target, late) {
        let touch_palm = last.mode == "palm"
            && state.active_palms[idx].as_ref().is_some_and(|palm| {
                (palm.end_seconds - last.end_seconds).abs() < EPS
                    && palm
                        .covered_note_ids
                        .iter()
                        .all(|id| chart.notes.iter().any(|n| &n.id == id && n.kind == "touch"))
            });
        if c.glide_distance > 0.0
            && (last.mode == "tap" || touch_palm)
            && distance > EPS
            && distance <= c.glide_distance
            && time >= last.start_seconds + JUDGE_FRAME - EPS
            && time - last.start_seconds < c.repetition_seconds - EPS
        {
            let mut next = state.clone();
            let from = last.samples[0].point();
            let samples = vec![
                MotionSample::new(last.start_seconds, from),
                MotionSample::new(time, target),
            ];
            if replace_last(&mut next, idx, hand, samples, "glide", c).is_some() {
                if touch_palm {
                    if let Some(palm) = next.active_palms[idx].as_mut() {
                        palm.end_seconds = time;
                    }
                }
                out.push(Relief {
                    state: next,
                    begin: time,
                    glided: true,
                });
            }
        }
    }

    if last.mode == "hold" {
        let held = chart
            .notes
            .iter()
            .position(|n| Some(&n.id) == last.note_id.as_ref());
        if let Some(held) = held {
            let release = earliest_release(&chart.notes[held], c).max(last.start_seconds);
            let on_time = target.is_none() || arrive(release) <= time + EPS;
            let begin = if on_time && !late {
                Some(time)
            } else if !on_time && late && touch && arrive(release) <= late_limit + EPS {
                Some(arrive(release))
            } else {
                None
            };
            if let (Some(begin), true) = (begin, release < last.end_seconds - EPS) {
                let mut next = state.clone();
                let samples = vec![
                    MotionSample::new(last.start_seconds, point),
                    MotionSample::new(release, point),
                ];
                if replace_last(&mut next, idx, hand, samples, "hold", c).is_some() {
                    if next.held_touch[idx] == Some(held) {
                        next.held_touch[idx] = None;
                    }
                    out.push(Relief {
                        state: next,
                        begin,
                        glided: false,
                    });
                }
            }
        }
    }

    if late && touch && target.is_some() && arrive(arm.free) <= late_limit + EPS {
        out.push(Relief {
            state: state.clone(),
            begin: arrive(arm.free),
            glided: false,
        });
    }
    out
}

fn circumcenter(a: Point, b: Point, c: Point) -> Option<Point> {
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if d.abs() <= EPS {
        return None;
    }
    let (aa, bb, cc) = (
        a.x * a.x + a.y * a.y,
        b.x * b.x + b.y * b.y,
        c.x * c.x + c.y * c.y,
    );
    Some(Point {
        x: (aa * (b.y - c.y) + bb * (c.y - a.y) + cc * (a.y - b.y)) / d,
        y: (aa * (c.x - b.x) + bb * (a.x - c.x) + cc * (b.x - a.x)) / d,
    })
}

/// 圓形手掌的候選掌心來自落點、兩點中點及三點外接圓心。
/// 圓盤交集若非空，最小包圍圓必由其中 1–3 個落點決定，故能找到可行覆蓋組。
fn palm_candidates(group: &[Task], chart: &Chart, radius: f64) -> Vec<PalmCandidate> {
    if radius <= 0.0 {
        return vec![];
    }
    let touch: Vec<(usize, Point)> = group
        .iter()
        .enumerate()
        .filter(|(_, task)| matches!(chart.notes[task.note].kind.as_str(), "touch" | "touchHold"))
        .map(|(i, task)| (i, chart.notes[task.note].position))
        .collect();
    if touch.len() < 2 {
        return vec![];
    }
    let mut positions = Vec::<Point>::new();
    for (_, p) in &touch {
        if positions.iter().all(|q| p.distance(*q) > EPS) {
            positions.push(*p);
        }
    }
    let mut centers = vec![Point { x: 0.0, y: 0.0 }];
    centers.extend(positions.iter().copied());
    for (i, &a) in positions.iter().enumerate() {
        for (j, &b) in positions.iter().enumerate().skip(i + 1) {
            centers.push(a.lerp(b, 0.5));
            for &p in positions.iter().skip(j + 1) {
                if let Some(center) = circumcenter(a, b, p) {
                    centers.push(center);
                }
            }
        }
    }
    let mut by_coverage = BTreeMap::<Vec<usize>, Vec<Point>>::new();
    for center in centers {
        if center.x.hypot(center.y) > 1.0 + EPS {
            continue;
        }
        let covered: Vec<usize> = touch
            .iter()
            .filter(|(_, p)| center.distance(*p) <= radius + EPS)
            .map(|(i, _)| *i)
            .collect();
        if covered.len() < 2 {
            continue;
        }
        if covered.iter().all(|i| {
            chart.notes[group[*i].note]
                .position
                .distance(chart.notes[group[covered[0]].note].position)
                <= EPS
        }) {
            // 完全同位置的 Touch 已由單點接觸合併，無須顯示手掌動作。
            continue;
        }
        let variants = by_coverage.entry(covered).or_default();
        if variants.len() < 16 && variants.iter().all(|p| p.distance(center) > EPS) {
            variants.push(center);
        }
    }
    by_coverage
        .into_iter()
        .map(|(covered, centers)| PalmCandidate { covered, centers })
        .collect()
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

/// 把路徑的指定比例壓進實際時間區間。用於最後一小段提前掃完；起點仍沿用
/// 原本的名目位置，因此和上一段軌跡連續，終點則提早抵達 u=1。
fn path_samples_range(
    path: &SlidePath,
    u0: f64,
    u1: f64,
    start: f64,
    end: f64,
) -> Vec<MotionSample> {
    let mut samples = vec![MotionSample::new(start, path.at(u0))];
    for p in &path.samples {
        if p.u > u0 + EPS && p.u < u1 - EPS {
            let time = start + (p.u - u0) / (u1 - u0) * (end - start);
            samples.push(MotionSample::new(time, Point { x: p.x, y: p.y }));
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
        // 引導星星進入最後判定區的時刻，讓手可以恰好在這裡完成 Slide 尾判。
        let path = chart
            .paths
            .iter()
            .find(|p| Some(&p.id) == n.path_id.as_ref());
        if let Some(path) = path {
            let judge = start + (end - start) * path.judge_progress;
            if judge > start + EPS && judge < end - EPS {
                times.push(judge);
            }
        }
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
            // 長度 0 的 Hold（省略 `[...]`）仍要按下去，至少停留一般敲擊的接觸時間。
            n.end_seconds.max(n.time_seconds + c.contact_seconds)
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
                release_early: false,
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
            let contact_at_end = chart.notes.iter().enumerate().any(|(other, note)| {
                other != i && note.has_head && (note.time_seconds - end).abs() < EPS
            });
            for (k, pair) in times.windows(2).enumerate() {
                let last = k + 1 == steps;
                tasks.push(Task {
                    note: i,
                    start: pair[0],
                    end: pair[1],
                    mode: "slide",
                    samples: vec![],
                    path: path_index,
                    last,
                    path_length: length,
                    span: end - start,
                    continuation: k > 0,
                    release_early: last && contact_at_end,
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

/// 指派單一任務。除了原本的時序，手被上一組佔住時再加入判定規則允許的空手方式。
/// late=true 只回傳 Touch 在判定區間內晚接觸的指派。
fn assign(
    state: &State,
    task: &Task,
    hand: Hand,
    chart: &Chart,
    c: &SolverConfig,
    late: bool,
) -> Vec<State> {
    let mut out: Vec<State> = if late {
        vec![]
    } else {
        assign_once(state, task, hand, chart, c, None)
            .into_iter()
            .collect()
    };
    let idx = hand.index();
    let n = &chart.notes[task.note];
    let reliefs = if task.mode == "slide" {
        if late || state.engaged.contains_key(&task.note) {
            return out;
        }
        relieve(state, idx, hand, task.start, None, false, false, chart, c)
    } else {
        let touch = matches!(n.kind.as_str(), "touch" | "touchHold");
        relieve(
            state,
            idx,
            hand,
            task.start,
            Some(n.position),
            touch,
            late,
            chart,
            c,
        )
    };
    for relief in reliefs {
        let contact = (task.mode != "slide").then_some((relief.begin, relief.glided));
        out.extend(assign_once(&relief.state, task, hand, chart, c, contact));
    }
    out
}

/// contact 指定非 Slide 任務的實際接觸開始時間，以及是否已由上一個接觸滑過來。
fn assign_once(
    state: &State,
    task: &Task,
    hand: Hand,
    chart: &Chart,
    c: &SolverConfig,
    contact: Option<(f64, bool)>,
) -> Option<State> {
    let idx = hand.index();
    let n = &chart.notes[task.note];
    let glided = contact.is_some_and(|(_, glided)| glided);

    // Slide 的移動起點只是星星出發的時刻，不是手一定要貼上去的時刻。
    // 手可以晚一點才接上，剩下的路徑就壓縮在剩餘時間內走完；走得越急，速度成本越高。
    let engaged = state.engaged.get(&task.note).copied();
    let nominal_finish = n.motion_end;
    let early_finish = if task.release_early {
        let margin = c.slide_pickup_seconds.min((task.end - task.start) * 0.5);
        Some(task.end - margin)
    } else {
        None
    };
    let task_end = early_finish.unwrap_or(task.end);
    let (begin, pickup) = if task.mode == "slide" {
        let finish = early_finish.or(nominal_finish).unwrap();
        let pickup = engaged.unwrap_or_else(|| task.start.max(state.arms[idx].free));
        let begin = task.start.max(pickup);
        if begin >= task_end - EPS || pickup >= finish - EPS {
            return None;
        }
        if engaged.is_none() && pickup > n.motion_start.unwrap() + c.slide_pickup_seconds + EPS {
            return None;
        }
        (begin, Some(pickup))
    } else {
        (contact.map_or(task.start, |(begin, _)| begin), None)
    };
    // 晚接觸的 Touch 仍停留同樣長度；Hold 的結束時間不變。
    let shifted = task.mode != "slide" && begin > task.start + EPS;
    let task_end = if shifted && task.mode == "tap" {
        task_end + (begin - task.start)
    } else {
        task_end
    };
    if shifted && task_end <= begin + EPS {
        return None;
    }
    let samples = match (pickup, early_finish) {
        (Some(pickup), Some(finish)) => {
            let nominal_end = n.motion_end.unwrap();
            let u0 = (begin - pickup) / (nominal_end - pickup);
            path_samples_range(&chart.paths[task.path], u0, 1.0, begin, finish)
        }
        (Some(pickup), None) => path_samples(
            &chart.paths[task.path],
            pickup,
            n.motion_end.unwrap(),
            begin,
            task_end,
        ),
        (None, _) if shifted => vec![
            MotionSample::new(begin, n.position),
            MotionSample::new(task_end, n.position),
        ],
        (None, _) => task.samples.clone(),
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
        if !c.allow_handover || task_end - task.start + EPS < c.handover_seconds {
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
        && !glided
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
        if let (Some(t), false) = (a.last_tap, glide || glided) {
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
        end_seconds: task_end,
    });
    if n.kind == "touchHold" {
        next.held_touch[idx] = Some(task.note);
    }
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

fn assign_palm(
    state: &State,
    candidate: &PalmCandidate,
    group: &[Task],
    hand: Hand,
    chart: &Chart,
    c: &SolverConfig,
    late: bool,
) -> Vec<State> {
    let Some(first) = candidate.covered.first() else {
        return vec![];
    };
    let time = group[*first].start;
    let idx = hand.index();
    let arm = &state.arms[idx];
    // 同一覆蓋組保留多個可行掌心；選目前手最容易抵達的那個。
    let Some(center) = candidate
        .centers
        .iter()
        .min_by(|a, b| arm.point.distance(**a).total_cmp(&arm.point.distance(**b)))
        .copied()
    else {
        return vec![];
    };
    if arm.free <= time + EPS {
        if late {
            return vec![];
        }
        return palm_once(state, candidate, center, group, hand, chart, c, time, false)
            .into_iter()
            .collect();
    }
    // 手掌只覆蓋 Touch／Touch Hold，因此都可套用 Touch 的晚接觸區間。
    relieve(state, idx, hand, time, Some(center), true, late, chart, c)
        .into_iter()
        .filter_map(|relief| {
            palm_once(
                &relief.state,
                candidate,
                center,
                group,
                hand,
                chart,
                c,
                relief.begin,
                relief.glided,
            )
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn palm_once(
    state: &State,
    candidate: &PalmCandidate,
    center: Point,
    group: &[Task],
    hand: Hand,
    chart: &Chart,
    c: &SolverConfig,
    start: f64,
    glided: bool,
) -> Option<State> {
    let idx = hand.index();
    let arm = &state.arms[idx];
    if arm.free > start + EPS {
        return None;
    }
    if start - arm.free <= EPS && arm.point.distance(center) > EPS {
        return None;
    }
    let time = group[*candidate.covered.first()?].start;
    let shift = start - time;
    let contact_end = |i: &usize| {
        let task = &group[*i];
        if task.mode == "tap" {
            task.end + shift
        } else {
            task.end
        }
    };
    let end = candidate
        .covered
        .iter()
        .map(contact_end)
        .fold(start, f64::max);
    if end <= start + EPS {
        return None;
    }
    let mut next = state.clone();
    let a = &mut next.arms[idx];
    if start > a.free + EPS {
        let mode = if a.point.distance(center) < EPS {
            "idle"
        } else {
            "travel"
        };
        add_segment(
            a,
            hand,
            vec![
                MotionSample::new(a.free, a.point),
                MotionSample::new(start, center),
            ],
            mode,
            None,
            &mut next.cost,
            c,
        );
    }
    if let (Some(last), false) = (a.last_tap, glided) {
        next.cost.repetition += c.repetition_weight
            * (1.0 - (time - last) / c.repetition_seconds)
                .max(0.0)
                .powi(2);
    }
    a.last_tap = Some(time);
    let note_ids: Vec<String> = candidate
        .covered
        .iter()
        .map(|i| chart.notes[group[*i].note].id.clone())
        .collect();
    add_segment(
        a,
        hand,
        vec![
            MotionSample::new(start, center),
            MotionSample::new(end, center),
        ],
        "palm",
        note_ids.first().cloned(),
        &mut next.cost,
        c,
    );
    for (i, note_id) in candidate.covered.iter().zip(&note_ids) {
        next.assignments = next.assignments.push(Assignment {
            note_id: note_id.clone(),
            part: "contact".into(),
            hand,
            start_seconds: start,
            end_seconds: contact_end(i),
        });
    }
    if let Some(previous) = next.active_palms[idx].take() {
        next.palms = next.palms.push(previous);
    }
    next.held_touch[idx] = None;
    next.active_palms[idx] = Some(PalmPlacement {
        hand,
        center,
        radius: c.palm_radius,
        start_seconds: start,
        end_seconds: end,
        covered_note_ids: note_ids,
    });
    Some(next)
}

/// 已按住中央／內圈 Touch Hold 時，後續落在同一掌範圍內的 Touch 可以由同一隻手掌
/// 繼續覆蓋。這讓 C 先出現、B 區稍後依序出現的配置不必虛構第三隻手。
fn extend_palm_to_touch(
    state: &State,
    task: &Task,
    hand: Hand,
    chart: &Chart,
    c: &SolverConfig,
) -> Option<State> {
    if c.palm_radius <= 0.0 || task.mode == "slide" {
        return None;
    }
    let note = &chart.notes[task.note];
    if !matches!(note.kind.as_str(), "touch" | "touchHold") {
        return None;
    }
    let idx = hand.index();
    let (center, mut placement, expected_mode) = if let Some(palm) = &state.active_palms[idx] {
        if palm.end_seconds < task.start - EPS
            || palm.center.distance(note.position) > palm.radius + EPS
        {
            return None;
        }
        (palm.center, palm.clone(), "palm")
    } else {
        let held_index = state.held_touch[idx]?;
        let held = &chart.notes[held_index];
        if held.end_seconds < task.start - EPS
            || held.position.distance(note.position) > c.palm_radius + EPS
        {
            return None;
        }
        (
            held.position,
            PalmPlacement {
                hand,
                center: held.position,
                radius: c.palm_radius,
                start_seconds: held.time_seconds,
                end_seconds: held.end_seconds,
                covered_note_ids: vec![held.id.clone()],
            },
            "hold",
        )
    };

    let mut next = state.clone();
    let (rest, previous) = next.arms[idx].segments.pop()?;
    if previous.mode != expected_mode
        || previous.end_seconds < task.start - EPS
        || previous
            .samples
            .iter()
            .any(|sample| sample.point().distance(center) > EPS)
    {
        return None;
    }
    let (distance, speed, side) = segment_terms(&previous.samples, hand, c);
    next.cost.distance -= distance;
    next.cost.speed -= speed;
    next.cost.side -= side;
    next.arms[idx].segments = rest;

    if !placement.covered_note_ids.contains(&note.id) {
        placement.covered_note_ids.push(note.id.clone());
    }
    placement.end_seconds = placement.end_seconds.max(task.end);
    add_segment(
        &mut next.arms[idx],
        hand,
        vec![
            MotionSample::new(placement.start_seconds, center),
            MotionSample::new(placement.end_seconds, center),
        ],
        "palm",
        previous.note_id,
        &mut next.cost,
        c,
    );
    next.assignments = next.assignments.push(Assignment {
        note_id: note.id.clone(),
        part: "contact".into(),
        hand,
        start_seconds: task.start,
        end_seconds: task.end,
    });
    next.held_touch[idx] = None;
    next.active_palms[idx] = Some(placement);
    Some(next)
}

/// Touch 沒有 Fast 判定，且只看感應區是否被按著。手若在 Critical Perfect 區間內
/// 貼著面板經過落點附近（追 Slide、滑移、按住 Hold 或停留的 Tap），這個 Touch
/// 就順帶完成，不另外產生動作段。只檢查已經排定的動作，不預測之後的軌跡。
fn brush_touch(
    state: &State,
    task: &Task,
    hand: Hand,
    chart: &Chart,
    c: &SolverConfig,
) -> Option<State> {
    let note = &chart.notes[task.note];
    if note.kind != "touch" || task.mode != "tap" || c.palm_radius <= 0.0 {
        return None;
    }
    let radius = TOUCH_BRUSH_RADIUS.min(c.palm_radius);
    let window = TOUCH_LATE_FRAMES * JUDGE_FRAME;
    let (lo, hi) = (task.start - window, task.start + window);
    let target = note.position;
    let mut best: Option<(f64, f64)> = None;
    let mut cursor = state.arms[hand.index()].segments.head.as_ref();
    while let Some(link) = cursor {
        let segment = &link.value;
        cursor = link.prev.head.as_ref();
        if segment.end_seconds < lo - EPS {
            break;
        }
        if segment.start_seconds > hi + EPS
            || !matches!(
                segment.mode.as_str(),
                "slide" | "handover" | "glide" | "hold" | "tap"
            )
        {
            continue;
        }
        // 外圈按鍵在螢幕外；按住或滑過按鍵的手不會碰到螢幕上的感應區。
        let on_screen = |p: Point| p.x.hypot(p.y) < SCREEN_EDGE;
        let tracking = matches!(segment.mode.as_str(), "slide" | "handover");
        for pair in segment.samples.windows(2) {
            let (t0, t1) = (pair[0].time_seconds, pair[1].time_seconds);
            let (a, b) = (pair[0].point(), pair[1].point());
            if !(tracking || on_screen(a) && on_screen(b)) {
                continue;
            }
            let (from, to) = (t0.max(lo), t1.min(hi));
            if to < from - EPS {
                continue;
            }
            let at = |t: f64| {
                if t1 - t0 <= EPS {
                    b
                } else {
                    a.lerp(b, ((t - t0) / (t1 - t0)).clamp(0.0, 1.0))
                }
            };
            let (p, q) = (at(from), at(to));
            let (dx, dy) = (q.x - p.x, q.y - p.y);
            let length = dx * dx + dy * dy;
            let u = if length <= EPS {
                0.0
            } else {
                (((target.x - p.x) * dx + (target.y - p.y) * dy) / length).clamp(0.0, 1.0)
            };
            let time = from + (to - from) * u;
            let distance = p.lerp(q, u).distance(target);
            if distance <= radius + EPS && best.is_none_or(|(d, _)| distance < d - EPS) {
                best = Some((distance, time));
            }
        }
    }
    let (_, time) = best?;
    let mut next = state.clone();
    next.assignments = next.assignments.push(Assignment {
        note_id: note.id.clone(),
        part: "contact".into(),
        hand,
        start_seconds: time,
        end_seconds: time + JUDGE_FRAME,
    });
    Some(next)
}

/// Slide 尾判：手依目前的接軌排程進入最後判定區的時刻落在 Critical Perfect 區間內，
/// 且 task.start 已不早於該時刻。正解時刻與區間寬度都隨形狀（judge_progress）與 Slide 長度改變。
fn slide_judged(state: &State, task: &Task, chart: &Chart) -> bool {
    let Some(&pickup) = state.engaged.get(&task.note) else {
        return false;
    };
    let note = &chart.notes[task.note];
    let (Some(start), Some(end)) = (note.motion_start, note.motion_end) else {
        return false;
    };
    let progress = chart.paths[task.path].judge_progress;
    let reach = pickup + (end - pickup) * progress;
    let (low, high) = slide_critical_window(start, end, progress);
    task.start >= reach - EPS && reach >= low - EPS && reach <= high + EPS
}

/// Slide 尾判 Critical Perfect 的絕對時間區間。
fn slide_critical_window(start: f64, end: f64, progress: f64) -> (f64, f64) {
    let judge = start + (end - start) * progress;
    let stay_frames = (end - judge) / JUDGE_FRAME;
    let late = (SLIDE_CRITICAL_FRAMES + stay_frames / 4.0).min(SLIDE_CRITICAL_MAX_FRAMES);
    let early = late.max(SLIDE_CRITICAL_FAST_FRAMES);
    (judge - early * JUDGE_FRAME, judge + late * JUDGE_FRAME)
}

/// MajdataPlay NoteLoader.cs 的 TOUCH_GROUPS：兩個判定區有共用邊才相鄰。
fn touch_adjacent(a: (char, u8), b: (char, u8)) -> bool {
    let near = |k: u8, d: i32| (((k as i32 - 1 + d).rem_euclid(8)) + 1) as u8;
    let one_way = |(area, k): (char, u8), other: (char, u8)| -> bool {
        let list: Vec<(char, u8)> = match area {
            'A' => vec![
                ('D', k),
                ('D', near(k, 1)),
                ('E', k),
                ('E', near(k, 1)),
                ('B', k),
            ],
            'D' => vec![('A', k), ('A', near(k, -1)), ('E', k)],
            'E' => vec![
                ('D', k),
                ('A', k),
                ('A', near(k, -1)),
                ('B', k),
                ('B', near(k, -1)),
            ],
            'B' => vec![
                ('E', k),
                ('E', near(k, 1)),
                ('B', near(k, -1)),
                ('B', near(k, 1)),
                ('A', k),
                ('C', 0),
            ],
            'C' => (1..=8).map(|i| ('B', i)).collect(),
            _ => vec![],
        };
        list.contains(&other)
    };
    one_way(a, b) || one_way(b, a)
}

/// 同一時刻、彼此相鄰的一般 Touch 連成 Touch Group；只保留至少 3 顆的組，
/// 兩顆的組過半就是全部，沒有連帶判定的效果。Touch Hold 不參與。
fn touch_groups(group: &[Task], chart: &Chart) -> Vec<Vec<usize>> {
    let sensor = |i: usize| {
        let note = &chart.notes[group[i].note];
        let area = note.touch_area.as_deref().and_then(|a| a.chars().next())?;
        Some(if area == 'C' {
            ('C', 0)
        } else {
            (area, note.button)
        })
    };
    let members: Vec<usize> = (0..group.len())
        .filter(|i| group[*i].mode == "tap" && chart.notes[group[*i].note].kind == "touch")
        .filter(|i| sensor(*i).is_some())
        .collect();
    let mut seen = vec![false; group.len()];
    let mut out = vec![];
    for &first in &members {
        if seen[first] {
            continue;
        }
        seen[first] = true;
        let mut component = vec![first];
        let mut k = 0;
        while k < component.len() {
            let here = sensor(component[k]).unwrap();
            for &other in &members {
                if !seen[other] && touch_adjacent(here, sensor(other).unwrap()) {
                    seen[other] = true;
                    component.push(other);
                }
            }
            k += 1;
        }
        if component.len() >= 3 {
            out.push(component);
        }
    }
    out
}

/// 整組指派完成時驗證 Touch Group：實際接觸必須超過半數，其餘才可連帶判定。
/// 連帶判定的 Touch 以最後一個實際接觸者的手記錄，part 為 "group"，不產生動作。
fn settle_touch_groups(
    mut state: State,
    auto: &[bool],
    touch_groups: &[Vec<usize>],
    group: &[Task],
    chart: &Chart,
) -> Option<State> {
    if !auto.iter().any(|a| *a) {
        return Some(state);
    }
    for members in touch_groups {
        if !members.iter().any(|i| auto[*i]) {
            continue;
        }
        let contacted: Vec<usize> = members.iter().copied().filter(|i| !auto[*i]).collect();
        if contacted.len() * 2 <= members.len() {
            return None;
        }
        let ids: Vec<&str> = contacted
            .iter()
            .map(|i| chart.notes[group[*i].note].id.as_str())
            .collect();
        let mut hand = None;
        let mut cursor = state.assignments.head.as_ref();
        while let Some(link) = cursor {
            if link.value.part != "slide" && ids.contains(&link.value.note_id.as_str()) {
                hand = Some(link.value.hand);
                break;
            }
            cursor = link.prev.head.as_ref();
        }
        let hand = hand?;
        for i in members.iter().filter(|i| auto[**i]) {
            let note = &chart.notes[group[*i].note];
            state.assignments = state.assignments.push(Assignment {
                note_id: note.id.clone(),
                part: "group".into(),
                hand,
                start_seconds: note.time_seconds,
                end_seconds: note.time_seconds + JUDGE_FRAME,
            });
        }
    }
    state.used_touch_group = true;
    state.lenient_in_group = true;
    Some(state)
}

fn is_touch_sweep_group(group: &[Task], chart: &Chart) -> bool {
    group.len() >= MIN_TOUCH_SWEEP_NOTES
        && group
            .iter()
            .all(|task| task.mode == "tap" && chart.notes[task.note].kind.as_str() == "touch")
}

/// 從目前手位出發，以最近鄰順序掃過半邊盤面的 Touch。相同距離時以音符索引
/// 固定順序，讓輸出在不同執行間保持一致。
fn order_touch_sweep_route(
    mut route: Vec<usize>,
    group: &[Task],
    chart: &Chart,
    mut point: Point,
) -> Vec<usize> {
    let mut ordered = Vec::with_capacity(route.len());
    while !route.is_empty() {
        let best = route
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                let pa = chart.notes[group[**a].note].position;
                let pb = chart.notes[group[**b].note].position;
                point
                    .distance(pa)
                    .total_cmp(&point.distance(pb))
                    .then_with(|| group[**a].note.cmp(&group[**b].note))
            })
            .map(|(index, _)| index)
            .unwrap();
        let task = route.remove(best);
        point = chart.notes[group[task].note].position;
        ordered.push(task);
    }
    ordered
}

/// 全盤級的大型同時 Touch 不是兩個固定掌心，而是兩隻手各掃過半邊面板。
/// 每個感應區仍有實際經過時間與 hand assignment；軌跡使用既有 glide 模式，
/// 因此前端不需要自行猜測一條額外路徑。
fn assign_touch_sweep(
    state: &State,
    group: &[Task],
    chart: &Chart,
    c: &SolverConfig,
    reversed: bool,
) -> Option<State> {
    if !is_touch_sweep_group(group, chart) {
        return None;
    }
    let judgment = group[0].start;
    let mut routes: [Vec<usize>; 2] = [vec![], vec![]];
    for (task_index, task) in group.iter().enumerate() {
        let point = chart.notes[task.note].position;
        let side = if point.x < -EPS {
            0
        } else if point.x > EPS {
            1
        } else if routes[0].len() <= routes[1].len() {
            0
        } else {
            1
        };
        routes[side ^ usize::from(reversed)].push(task_index);
    }
    if routes.iter().any(Vec::is_empty) {
        return None;
    }

    let mut next = state.clone();
    for hand in [Hand::L, Hand::R] {
        let idx = hand.index();
        let route_start = (judgment - TOUCH_SWEEP_SECONDS).max(next.arms[idx].free);
        if route_start >= judgment - EPS {
            return None;
        }
        if route_start > next.arms[idx].free + EPS {
            let arm = &mut next.arms[idx];
            add_segment(
                arm,
                hand,
                vec![
                    MotionSample::new(arm.free, arm.point),
                    MotionSample::new(route_start, arm.point),
                ],
                "idle",
                None,
                &mut next.cost,
                c,
            );
        }

        let route = order_touch_sweep_route(
            std::mem::take(&mut routes[idx]),
            group,
            chart,
            next.arms[idx].point,
        );
        let mut samples = vec![MotionSample::new(route_start, next.arms[idx].point)];
        let mut hits = Vec::with_capacity(route.len());
        for (step, task_index) in route.iter().enumerate() {
            let hit =
                route_start + (judgment - route_start) * (step + 1) as f64 / route.len() as f64;
            let note = &chart.notes[group[*task_index].note];
            samples.push(MotionSample::new(hit, note.position));
            hits.push((*task_index, hit));
        }
        let finish = hits
            .iter()
            .map(|(task_index, _)| group[*task_index].end)
            .fold(judgment, f64::max);
        let last_point = samples.last().unwrap().point();
        if finish > judgment + EPS {
            samples.push(MotionSample::new(finish, last_point));
        }
        let first_note = chart.notes[group[route[0]].note].id.clone();
        add_segment(
            &mut next.arms[idx],
            hand,
            samples,
            "glide",
            Some(first_note),
            &mut next.cost,
            c,
        );
        next.arms[idx].last_tap = Some(judgment);
        for (task_index, hit) in hits {
            let note = &chart.notes[group[task_index].note];
            next.assignments = next.assignments.push(Assignment {
                note_id: note.id.clone(),
                part: "contact".into(),
                hand,
                start_seconds: hit,
                end_seconds: (hit + c.contact_seconds).min(finish),
            });
        }
    }
    next.used_touch_sweep = true;
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

/// 先以「準時接觸優先」搜尋；若因此找不到方案，再允許所有 Touch 使用晚接觸重試一次。
/// 優先規則只是縮小候選的策略，可能剪掉必須晚接才走得通的路線。
pub fn solve(chart: &Chart, c: &SolverConfig) -> Result<Vec<Solution>, Diagnostic> {
    match solve_with(chart, c, false) {
        Err(first) if first.code == "no_solution" => {
            let mut solutions = solve_with(chart, c, true)?;
            for solution in &mut solutions {
                solution.warnings.push(
                    "準時接觸優先的搜尋無解，此方案允許 Touch 在判定區間內任意晚接觸。".into(),
                );
            }
            Ok(solutions)
        }
        other => other,
    }
}

fn solve_with(chart: &Chart, c: &SolverConfig, relaxed: bool) -> Result<Vec<Solution>, Diagnostic> {
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
    let v2_context = if c.is_v2() {
        Some(v2::Context::new(chart, c)?)
    } else {
        None
    };
    let mut beam = vec![State {
        v2: v2::Data::default(),
        group_origin: 0,
        arms,
        cost: CostBreakdown::default(),
        assignments: Chain::default(),
        handovers: Chain::default(),
        palms: Chain::default(),
        active_palms: [None, None],
        held_touch: [None, None],
        owners: BTreeMap::new(),
        engaged: BTreeMap::new(),
        pending: 0.0,
        deposit: BTreeMap::new(),
        last_handover: BTreeMap::new(),
        used_touch_sweep: false,
        lenient_in_group: false,
        finished: std::collections::BTreeSet::new(),
        used_early_slide: false,
        used_touch_group: false,
    }];
    if let Some(context) = &v2_context {
        context.refresh(&mut beam[0])?;
    }
    let tasks = tasks(chart, c)?;
    // 需要新接觸的時刻（Tap、Hold、Touch 與 Slide 起點），用來預判手是否需要提早離開 Slide。
    let mut onsets: Vec<f64> = chart
        .notes
        .iter()
        .filter(|n| n.has_head)
        .map(|n| n.time_seconds)
        .collect();
    onsets.sort_by(f64::total_cmp);
    let clock = Instant::now();
    let mut expansions: u64 = 0;
    let mut cursor = 0;
    while cursor < tasks.len() {
        let time = tasks[cursor].start;
        let mut limit = cursor + 1;
        while limit < tasks.len() && (tasks[limit].start - time).abs() < EPS {
            limit += 1;
        }
        let group = &tasks[cursor..limit];
        // 已經結束的 Slide 不會再被查詢；清掉之後每個狀態要複製的資料量才是常數。
        for (origin, state) in beam.iter_mut().enumerate() {
            state.group_origin = origin;
            state.lenient_in_group = false;
            let live =
                |note: &usize| chart.notes[*note].motion_end.unwrap_or(f64::INFINITY) >= time - EPS;
            state.owners.retain(|note, _| live(note));
            state.engaged.retain(|note, _| live(note));
            state.finished.retain(&live);
            state.last_handover.retain(|note, _| live(note));
            state.deposit.retain(|note, held| {
                let keep = live(note);
                if !keep {
                    state.pending -= *held;
                }
                keep
            });
            for idx in 0..2 {
                if state.active_palms[idx]
                    .as_ref()
                    .is_some_and(|palm| palm.end_seconds < time - EPS)
                {
                    let palm = state.active_palms[idx].take().unwrap();
                    state.palms = state.palms.push(palm);
                }
                if state.held_touch[idx]
                    .is_some_and(|note| chart.notes[note].end_seconds < time - EPS)
                {
                    state.held_touch[idx] = None;
                }
            }
        }
        // 兩手若在這一刻碰頭，先把「互換目的地」的變化加進 beam，再一起展開這個任務。
        let swapped: Vec<State> = beam
            .iter()
            .filter_map(|state| swap_at(state, time, chart, c))
            .collect();
        beam.extend(swapped);
        if is_touch_sweep_group(group, chart) {
            let mut swept = Vec::new();
            for state in &beam {
                for reversed in [false, true] {
                    if let Some(mut result) = assign_touch_sweep(state, group, chart, c, reversed) {
                        if let Some(context) = &v2_context {
                            context.update(state, &mut result)?;
                        }
                        swept.push(result);
                    }
                }
            }
            if !swept.is_empty() {
                beam = swept;
                prune(&mut beam, c);
                cursor = limit;
                continue;
            }
        }
        let candidates = palm_candidates(group, chart, c.palm_radius);
        let touch_groups = touch_groups(group, chart);
        let in_touch_group: Vec<bool> = (0..group.len())
            .map(|i| touch_groups.iter().any(|g| g.contains(&i)))
            .collect();
        let mut next = vec![];
        // 第三個欄位標記由 Touch Group 過半判定連帶完成、沒有實際接觸的 Touch。
        let mut pending: Vec<(State, Vec<bool>, Vec<bool>)> = beam
            .into_iter()
            .map(|state| (state, vec![true; group.len()], vec![false; group.len()]))
            .collect();
        while let Some((state, remaining, auto)) = pending.pop() {
            if expansions > MAX_EXPANSIONS
                || clock.elapsed() > BUDGET
                || pending.len() + next.len() > MAX_GROUP_STATES
            {
                return Err(Diagnostic::plain(
                    "search_limit",
                    "同時事件的候選已達計算預算，請縮短片段或降低 beamWidth".into(),
                ));
            }
            // Touch Group 成員最後展開：其他音符先分到手，剩下接不到的 Touch 才交給過半判定。
            let next_task = (0..group.len())
                .find(|j| remaining[*j] && !in_touch_group[*j])
                .or_else(|| remaining.iter().position(|todo| *todo));
            let Some(i) = next_task else {
                let Some(mut state) =
                    settle_touch_groups(state, &auto, &touch_groups, group, chart)
                else {
                    continue;
                };
                if let Some(context) = &v2_context {
                    context.refresh(&mut state)?;
                }
                next.push(state);
                continue;
            };
            let task = &group[i];
            let mut after = remaining.clone();
            after[i] = false;
            if task.mode == "slide" && state.finished.contains(&task.note) {
                pending.push((state, after, auto));
                continue;
            }
            // 準時的接觸優先；只有兩手都無法準時完成這顆音符時，才展開 Touch 在
            // Critical Perfect 區間內的晚接觸。relaxed 為整譜重試，一律展開。
            let before = pending.len();
            for late in [false, true] {
                if late && !relaxed && pending.len() > before {
                    break;
                }
                for hand in [Hand::L, Hand::R] {
                    expansions += 1;
                    for mut s in assign(&state, task, hand, chart, c, late) {
                        s.lenient_in_group |= late;
                        if let Some(context) = &v2_context {
                            context.update(&state, &mut s)?;
                        }
                        pending.push((s, after.clone(), auto.clone()));
                    }
                    if late {
                        continue;
                    }
                    expansions += 1;
                    if let Some(mut s) = extend_palm_to_touch(&state, task, hand, chart, c) {
                        if let Some(context) = &v2_context {
                            context.update(&state, &mut s)?;
                        }
                        pending.push((s, after.clone(), auto.clone()));
                    }
                    expansions += 1;
                    if let Some(mut s) = brush_touch(&state, task, hand, chart, c) {
                        if let Some(context) = &v2_context {
                            context.update(&state, &mut s)?;
                        }
                        pending.push((s, after.clone(), auto.clone()));
                    }
                }
                if !matches!(chart.notes[task.note].kind.as_str(), "touch" | "touchHold") {
                    continue;
                }
                for candidate in &candidates {
                    if !candidate.covered.contains(&i)
                        || candidate.covered.iter().any(|j| !remaining[*j])
                    {
                        continue;
                    }
                    let mut after_palm = remaining.clone();
                    for j in &candidate.covered {
                        after_palm[*j] = false;
                    }
                    for hand in [Hand::L, Hand::R] {
                        let variants: Vec<PalmCandidate> = if c.is_v2() {
                            candidate
                                .centers
                                .iter()
                                .map(|center| PalmCandidate {
                                    covered: candidate.covered.clone(),
                                    centers: vec![*center],
                                })
                                .collect()
                        } else {
                            vec![PalmCandidate {
                                covered: candidate.covered.clone(),
                                centers: candidate.centers.clone(),
                            }]
                        };
                        for variant in &variants {
                            expansions += 1;
                            for mut s in assign_palm(&state, variant, group, hand, chart, c, late) {
                                s.lenient_in_group |= late;
                                if let Some(context) = &v2_context {
                                    context.update(&state, &mut s)?;
                                }
                                pending.push((s, after_palm.clone(), auto.clone()));
                            }
                        }
                    }
                }
            }
            // 還沒接上的 Slide 可先不接；排序鍵預存其完整路徑的分攤成本。
            let deferrable = task.mode == "slide"
                && !task.last
                && task.end
                    <= chart.notes[task.note].motion_start.unwrap() + c.slide_pickup_seconds + EPS;
            // Late pickup resolves an occupied hand or another simultaneous
            // contact. It is not a way to shorten an uncontested Slide's
            // opposite-side exposure for free below the comfort threshold.
            let pickup_conflict = !c.is_v2()
                || after
                    .iter()
                    .enumerate()
                    .any(|(j, todo)| *todo && group[j].mode != "slide")
                || [Hand::L, Hand::R]
                    .iter()
                    .all(|hand| assign(&state, task, *hand, chart, c, false).is_empty());
            if deferrable && !state.engaged.contains_key(&task.note) && pickup_conflict {
                let dt = task.end - task.start;
                let span = task.span.max(EPS);
                let share = if let Some(context) = &v2_context {
                    context.deposit(task, chart)?
                } else {
                    c.distance_weight * task.path_length * dt / span
                        + c.speed_weight * task.path_length.powi(2) * dt
                            / span.powi(2)
                            / c.speed_reference.powi(2)
                };
                let mut deferred = state.clone();
                deferred.pending += share;
                *deferred.deposit.entry(task.note).or_default() += share;
                pending.push((deferred, after.clone(), auto.clone()));
            }
            // Slide 尾判只看手何時進入最後判定區；進入後若同組或 Slide 結束前還有別的音符
            // 需要手，可以不再追完剩下的路徑。這個選擇要在進入判定區的 checkpoint 就做，
            // 等衝突真的出現時手已經多追了一段，因此以「結束前有新音符」預判，不套用
            // 同組準時優先的過濾。
            let needed_elsewhere = task.mode == "slide"
                && (after.iter().enumerate().any(|(j, todo)| {
                    // 另一條已接上的 Slide 由原本的手繼續追，不算需要這隻手。
                    *todo
                        && group[j].note != task.note
                        && (group[j].mode != "slide" || !state.engaged.contains_key(&group[j].note))
                })
                    // 名目終點上的接觸已由最後一段提前掃完處理，這裡只看終點之前。
                    || chart.notes[task.note].motion_end.is_some_and(|end| {
                        let from = onsets.partition_point(|t| *t <= task.start + EPS);
                        onsets.get(from).is_some_and(|t| *t < end - EPS)
                    }));
            if task.mode == "slide"
                && (relaxed || needed_elsewhere)
                && slide_judged(&state, task, chart)
            {
                let mut done = state.clone();
                done.finished.insert(task.note);
                done.owners.remove(&task.note);
                done.engaged.remove(&task.note);
                done.used_early_slide = true;
                pending.push((done, after.clone(), auto.clone()));
            }
            // 沒有任何一手接得到的 Touch，交給 Touch Group 過半判定；整組結束時再驗證過半。
            if pending.len() == before && in_touch_group[i] {
                let mut grouped = auto.clone();
                grouped[i] = true;
                pending.push((state, after, grouped));
            }
            if expansions > MAX_EXPANSIONS
                || clock.elapsed() > BUDGET
                || pending.len() + next.len() > MAX_GROUP_STATES
            {
                return Err(Diagnostic::plain(
                    "search_limit",
                    "同時事件的候選已達計算預算，請縮短片段或降低 beamWidth".into(),
                ));
            }
        }
        if next.is_empty() {
            let mut d = Diagnostic::plain(
                "no_solution",
                "本模型未找到可行方案：兩手在此時都被佔用，或無法在判定區間內連續接觸。這不代表人類無法遊玩；可嘗試增加 beamWidth、palmRadius 或 glideDistance，或縮短片段。".into(),
            );
            d.time_seconds = Some(time);
            d.note_ids = group
                .iter()
                .map(|t| chart.notes[t.note].id.clone())
                .collect();
            d.note_ids.sort();
            d.note_ids.dedup();
            d.source_span = Some(Box::new(chart.notes[group[0].note].source_span.clone()));
            return Err(d);
        }
        if c.is_v2() {
            v2::remove_unnecessary_deferrals(&mut next);
        }
        if !relaxed {
            // 逐顆判斷時，前面的手掌可能只覆蓋部分 Touch，把剩下的推成晚接觸或連帶判定。
            // 同一個 beam 狀態若能整組準時、實際接觸完成，就不保留這類退路分支。
            let on_time: std::collections::BTreeSet<usize> = next
                .iter()
                .filter(|s| !s.lenient_in_group)
                .map(|s| s.group_origin)
                .collect();
            next.retain(|s| !s.lenient_in_group || !on_time.contains(&s.group_origin));
        }
        prune(&mut next, c);
        beam = next;
        cursor = limit;
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
        if let Some(context) = &v2_context {
            context.refresh(state)?;
            context.verify(state)?;
        } else {
            let left = state.arms[0].segments.to_vec();
            let right = state.arms[1].segments.to_vec();
            state.cost.cross = cross_cost(&left, &right, c);
        }
    }
    beam.sort_by(|a, b| v2::compare(a, b, c.is_v2(), true));
    let mut solutions = vec![];
    let mut signatures = std::collections::BTreeSet::new();
    for state in beam {
        let assignments = merge_assignments(state.assignments.to_vec());
        let signature = serde_json::to_string(&assignments).unwrap();
        if !signatures.insert(signature) {
            continue;
        }
        let mut palm_placements = state.palms.to_vec();
        palm_placements.extend(state.active_palms.iter().flatten().cloned());
        palm_placements.sort_by(|a, b| {
            a.start_seconds
                .total_cmp(&b.start_seconds)
                .then_with(|| a.hand.index().cmp(&b.hand.index()))
        });
        let used_touch_sweep = state.used_touch_sweep;
        let (used_early_slide, used_touch_group) = (state.used_early_slide, state.used_touch_group);
        let [left, right] = state.arms;
        let mut warnings = vec![
            "單點與圓形手掌的 Demo 幾何近似，未模擬實機感測器判定或手臂關節。".into(),
            "Beam Search 不保證全域最優；交叉成本於候選完整後排序，搜尋剪枝依其他成本。".into(),
        ];
        if c.is_v2() {
            warnings[1] = "直覺優先 V2：搜尋已包含活動接觸交叉；Beam Search 不保證全域最佳，分數不是人類使用機率。".into();
        }
        if used_touch_sweep {
            warnings.push(
                "大型同時 Touch 以判定前 0.18 秒的雙手連續掃屏近似；這不是官方判定窗。".into(),
            );
        }
        if used_early_slide {
            warnings.push(
                "部分 Slide 在手進入最後判定區後就離手（尾判只看進入時刻），畫面上的手不會追到終點。".into(),
            );
        }
        if used_touch_group {
            warnings.push(
                "部分 Touch 由 Touch Group 過半判定連帶完成，沒有實際接觸（標示為「Group 連帶」）。".into(),
            );
        }
        solutions.push(Solution {
            id: format!("solution-{}", solutions.len() + 1),
            total_cost: state.cost.total(),
            cost_breakdown: state.cost,
            score: if c.is_v2() { state.v2.score } else { None },
            score_breakdown: if c.is_v2() {
                Some(state.v2.parts)
            } else {
                None
            },
            scoring_model: if c.is_v2() {
                Some(c.scoring_model.clone())
            } else {
                None
            },
            assignments,
            handovers: state.handovers.to_vec(),
            palm_placements,
            left_segments: left.segments.to_vec(),
            right_segments: right.segments.to_vec(),
            config_snapshot: c.clone(),
            warnings,
        });
        if solutions.len() == c.top_k {
            break;
        }
    }
    Ok(solutions)
}

/// Preserve a few distinct active owner/deferred states within the same budget.
fn prune(states: &mut Vec<State>, c: &SolverConfig) {
    states.sort_by(|a, b| v2::compare(a, b, c.is_v2(), false));
    if !c.is_v2() || states.len() <= c.beam_width || c.beam_width < 4 {
        states.truncate(c.beam_width);
        return;
    }
    let quota = (c.beam_width / 4).min(8);
    let mut seen = std::collections::BTreeSet::new();
    let mut selected = std::collections::BTreeSet::new();
    for (i, state) in states.iter().enumerate() {
        let signature = (
            state
                .owners
                .iter()
                .map(|(n, h)| (*n, h.index()))
                .collect::<Vec<_>>(),
            state.deposit.keys().copied().collect::<Vec<_>>(),
        );
        if seen.insert(signature) {
            selected.insert(i);
        }
        if selected.len() >= quota {
            break;
        }
    }
    for i in 0..states.len() {
        if selected.len() >= c.beam_width {
            break;
        }
        selected.insert(i);
    }
    let mut index = 0;
    states.retain(|_| {
        let keep = selected.contains(&index);
        index += 1;
        keep
    });
}
