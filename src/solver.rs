mod tracking;
mod v2;
mod v3;
#[doc(hidden)]
pub use v3::phrases::{
    auto_group_keys, motion_group_keys, phrase_shapes, set_templates, with_groups,
};

/// 規則代碼（Temoto 分組模型的特徵與整組套用共用）：0 全同手、1 交替、2 同手或交替。
fn rule_code(rule: v3::phrases::GroupRule) -> u8 {
    use v3::phrases::GroupRule;
    match rule {
        GroupRule::Same => 0,
        GroupRule::Alternate => 1,
        GroupRule::SameOrAlternate => 2,
    }
}

/// Temoto 分組模型的特徵：每個自動分組（classify 後、依時間排序）、classify 的規則，
/// 以及 auto_rules 的規則（沒有成本的組為 None）。模型是用拍點切法訓練的，一律用拍點切法。
pub(crate) fn auto_group_info(chart: &Chart) -> Vec<(Vec<&Note>, u8, Option<u8>)> {
    use v3::phrases::{auto_groups, auto_rules, classify, GroupCut};
    let auto: BTreeMap<&str, u8> = auto_rules(chart, GroupCut::Beats)
        .into_iter()
        .map(|(notes, rule, _)| (notes[0].key.as_str(), rule_code(rule)))
        .collect();
    auto_groups(chart, GroupCut::Beats)
        .into_iter()
        .filter_map(classify)
        .map(|(notes, rule)| {
            let first = auto.get(notes[0].key.as_str()).copied();
            (notes, rule_code(rule), first)
        })
        .collect()
}

/// 研究用（訓練 Temoto 分組模型的資料）：[`auto_group_info`] 的音符改成 key。
#[doc(hidden)]
pub fn auto_group_rules(chart: &Chart) -> Vec<(Vec<String>, u8, Option<u8>)> {
    auto_group_info(chart)
        .into_iter()
        .map(|(notes, rule, auto)| (notes.iter().map(|n| n.key.clone()).collect(), rule, auto))
        .collect()
}

/// Temoto 整組套用：有規則（auto_rules）的自動分組與規則代碼（拍點切法，同 [`auto_group_info`]）。
pub(crate) fn rule_groups(chart: &Chart) -> Vec<(Vec<&Note>, u8)> {
    v3::phrases::auto_rules(chart, v3::phrases::GroupCut::Beats)
        .into_iter()
        .map(|(notes, rule, _)| (notes, rule_code(rule)))
        .collect()
}
#[cfg(test)]
mod v3_tests;
use crate::geometry::button;
use crate::scoring_v3::{ActionEvent, ActionKind};
use crate::*;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const EPS: f64 = 1e-8;

thread_local! {
    /// 這個執行緒上正在跑的分析的取消旗標；搜尋迴圈定期檢查。
    static CANCEL: RefCell<Option<Arc<AtomicBool>>> = const { RefCell::new(None) };
}

/// 這個執行緒目前的取消旗標；在別的執行緒上接著跑同一份工作時用 [`with_cancel`] 帶過去。
pub(crate) fn current_cancel() -> Option<Arc<AtomicBool>> {
    CANCEL.with(|slot| slot.borrow().clone())
}

/// 在 `flag` 被設為 true 時中止這次求解（求解回傳 code 為 `cancelled` 的診斷）。
/// 只影響目前執行緒，結束後恢復原狀。
pub fn with_cancel<T>(flag: Arc<AtomicBool>, run: impl FnOnce() -> T) -> T {
    let previous = CANCEL.with(|slot| slot.replace(Some(flag)));
    let result = run();
    CANCEL.with(|slot| *slot.borrow_mut() = previous);
    result
}

fn cancelled() -> bool {
    CANCEL.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
    })
}

/// 計算預算。狀態複製已是常數成本，上限因此由整體工作量與時間決定，
/// 不再靠音符數量的硬性上限來遮掩搜尋成本。
/// MAX_TASKS 同時是記憶體防線：每個事件在 beam 中約佔 26 KB，
/// 實測一首完整譜面（857 個音符、約 1100 個事件）峰值只有 19 MB。
const MAX_TASKS: usize = 50_000;
/// 兩條 Slide 要多近才算「碰頭」。必須近到可以視為同一點，兩手才能原地互換目的地
/// 而不產生軌跡跳動；只是空間交叉、時間錯開的不算。
const MEET_TOLERANCE: f64 = 1e-6;
/// 兩條 Slide 的星星要多近才算重疊、可由同一隻手一起畫（盤面半徑 1）。同一圈上同方向等速的
/// 星星只差路徑取樣的折線誤差；這個距離遠小於感應區，是 Demo 假設。
const CARRY_TOLERANCE: f64 = 0.05;
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
/// （B5–E6 為 0.279）：經過鄰區中心會碰到，但不會跨到 B–B（0.356）或 A–D（0.333）
/// 的下一區。是 Demo 假設，不是實機感應區面積。
const TOUCH_BRUSH_RADIUS: f64 = 0.28;
/// Slide 尾判 Critical Perfect 的基本半寬與最大半寬（幀）；中央區間向兩側各擴展
/// 「引導星星在最後判定區停留時間 / 4」，另有 −17～−14 幀的 Critical Perfect。
const SLIDE_CRITICAL_FRAMES: f64 = 14.0;
const SLIDE_CRITICAL_MAX_FRAMES: f64 = 36.0;
const SLIDE_CRITICAL_FAST_FRAMES: f64 = 17.0;
/// 按鍵代表點在半徑 1、最外圈感應區在 0.9；介於兩者之間視為螢幕邊界。
const SCREEN_EDGE: f64 = 0.95;
/// V3 暫停 Slide：剩下的滑行時間至少這麼長才考慮放開（長 Slide 期間去打別的音符）；
/// 短 Slide 放開再接回只是規避追蹤成本的捷徑，不是真人打法。
const SUSPEND_MIN_REMAINING: f64 = 1.0;
/// Slide 交給後一條一筆畫完時，手畫到下一個要接觸的音符前這麼多秒內才放開，
/// 不為了少畫一段提早離手。
const SWEEP_LEAD: f64 = 0.5;
/// 接回暫停的 Slide 時，剩下的路徑最快能以這個速度掃完（盤面半徑為 1，每秒；約為 speedReference 預設的 3 倍）。
const RESUME_MAX_SPEED: f64 = 12.0;
/// 照標註略過模式的退路：只記這麼多秒內、最多這麼多個岔路，整首最多退回這麼多次。
const FORK_WINDOW: f64 = 4.0;
const MAX_FORKS: usize = 64;
const MAX_BACKTRACKS: usize = 32;

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
    v3: v3::Data,
    actions: Chain<ActionEvent>,
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
    /// WiFi: owner covers the center plus this side; the other hand covers the other side.
    wifi_pairs: BTreeMap<usize, usize>,
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
    /// V3：追到一半先放開的 Slide → (判定已推進到的路徑比例 u, 目前記在預存裡的接回壓縮下限)。
    /// 判定進度會保留（MajdataPlay SlideBase.SensorCheck），之後任一手可從這裡接回去，在名目終點前追完。
    suspended: BTreeMap<usize, (f64, f64)>,
    /// 已經暫停過又接回去的 Slide；每條只允許暫停一次，避免搜尋分支暴增。
    resumed: std::collections::BTreeSet<usize>,
    /// V3：與另一條同時 Slide 由同一隻手張開覆蓋。bundled 為「被帶著走的 Slide → 主導的 Slide」，
    /// bundle_lead 反過來。被帶著走的那條不再有自己的 checkpoint 接觸。
    bundled: BTreeMap<usize, usize>,
    bundle_lead: BTreeMap<usize, usize>,
    used_bundle: bool,
    /// V3：星星重疊、由追著另一條的手一起畫的 Slide → 帶著它的 Slide。帶它的那條結束前，
    /// 被帶著的這條沒有自己的 checkpoint 接觸；之後由同一隻手接著畫自己的路徑。
    carried: BTreeMap<usize, usize>,
    /// 必須由目前的手繼續追到這個時刻的 Slide：正帶著重疊的 Slide，或已預先靠它順帶碰 Touch。
    /// 期間不交接、不互換、不暫停、不提早離手。
    committed: BTreeMap<usize, f64>,
    used_carry: bool,
    /// V3：畫到一半放開、剩下的判定區交給較晚出發的另一條 Slide 一筆畫完 → 那一條
    /// （見 tracking::sweeping）。之後沒有自己的 checkpoint 接觸。
    swept: BTreeMap<usize, usize>,
    used_sweep: bool,
    used_pair_press: bool,
    /// 這一組開始時的指派數；標註限制只檢查之後新增的指派。
    assign_mark: usize,
    /// 真人標註指定要換手的 Slide（音符 id）。V3 平常只在另一手有需求時才考慮交接，
    /// 這是縮小搜尋的近似，不是判定規則；標註明確要求換手時不套用。
    forced_handovers: Arc<std::collections::HashSet<String>>,
    /// 真人標註指定由單手滑的 Slide（音符 id）。WiFi 平常只在另一手抽不開時才單手張開，
    /// 標註明確指定單手時不套用。
    single_track: Arc<std::collections::HashSet<String>>,
    used_early_slide: bool,
    used_touch_group: bool,
    /// 部分解：走不下去時略過、不指派手的音符部分（最新的在前）。
    dropped: Chain<Dropped>,
}
/// 部分解在求解途中略過的音符部分（見 solve_partial）。
#[derive(Clone)]
struct Dropped {
    note: usize,
    /// true 是 Slide 的滑行，false 是接觸（Tap／Hold／Touch 與星星頭）。
    slide: bool,
    /// 走不下去的那一組的時刻。
    time: f64,
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

/// Hold／Touch Hold 最早可以放手而不影響判定的時間。關閉 holdEarlyRelease 時一律按到結尾。
fn earliest_release(note: &Note, c: &SolverConfig) -> f64 {
    if !c.hold_early_release {
        return note.end_seconds;
    }
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
        // V3: a hold that already ended may still be left at its earliest legal
        // release, so the hand is not forced into a burst move right at the end.
        // Slides pass no target (where the hand joins depends on the pickup time),
        // so a hold ending exactly as the track starts may also be released early.
        if c.is_v3() && !late {
            if let Some(last) = arm.segments.last() {
                if last.mode == "hold"
                    && target.is_none_or(|target| {
                        last.samples.last().unwrap().point().distance(target) > EPS
                    })
                {
                    if let Some(held) = chart
                        .notes
                        .iter()
                        .find(|n| Some(&n.id) == last.note_id.as_ref())
                    {
                        let release = earliest_release(held, c).max(last.start_seconds);
                        if release < last.end_seconds - EPS {
                            let mut next = state.clone();
                            let point = last.samples.last().unwrap().point();
                            let samples = vec![
                                MotionSample::new(last.start_seconds, point),
                                MotionSample::new(release, point),
                            ];
                            if replace_last(&mut next, idx, hand, samples, "hold", c).is_some() {
                                if next.held_touch[idx]
                                    .is_some_and(|n| chart.notes[n].id == held.id)
                                {
                                    next.held_touch[idx] = None;
                                }
                                out.push(Relief {
                                    state: next,
                                    begin: time,
                                    glided: false,
                                });
                            }
                        }
                    }
                }
            }
        }
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
    range_samples(&path.samples, u0, u1, start, end)
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
    range_samples(&path.samples, u0, u1, start, end)
}

fn range_samples(
    route: &[PathSample],
    u0: f64,
    u1: f64,
    start: f64,
    end: f64,
) -> Vec<MotionSample> {
    let mut samples = vec![MotionSample::new(start, sample_at(route, u0))];
    for p in route {
        if p.u > u0 + EPS && p.u < u1 - EPS {
            let time = start + (p.u - u0) / (u1 - u0) * (end - start);
            samples.push(MotionSample::new(time, Point { x: p.x, y: p.y }));
        }
    }
    samples.push(MotionSample::new(end, sample_at(route, u1)));
    samples
}

/// V3 共用路線（手掌覆蓋、WiFi）在這個 checkpoint 的取樣；規則與一般 Slide 相同：
/// 接上後等速走完剩餘路線，名目終點有新接觸時最後一段提前掃完。
fn route_track(
    route: &[PathSample],
    task: &Task,
    pickup: f64,
    nominal_end: f64,
    begin: f64,
    c: &SolverConfig,
) -> Option<Vec<MotionSample>> {
    let (end, u1) = if task.release_early {
        (
            task.end - c.slide_pickup_seconds.min((task.end - task.start) * 0.5),
            1.0,
        )
    } else {
        (task.end, (task.end - pickup) / (nominal_end - pickup))
    };
    if begin >= end - EPS || pickup >= nominal_end - EPS {
        return None;
    }
    let u0 = (begin - pickup) / (nominal_end - pickup);
    Some(range_samples(route, u0, u1, begin, end))
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
    // 被帶著畫的 Slide 在帶它的那條結束時接回自己的路徑，這一刻要切開。
    for (i, n) in chart.notes.iter().enumerate() {
        let (Some(start), Some(end)) = (n.motion_start, n.motion_end) else {
            continue;
        };
        let Some(path) = chart
            .paths
            .iter()
            .find(|p| Some(&p.id) == n.path_id.as_ref())
        else {
            continue;
        };
        for &lead in &path.hand_routes.carriers {
            let until = chart.notes[lead].motion_end.unwrap();
            if until > start + EPS && until < end - EPS {
                schedule[i].push(until);
            }
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
            .then_with(|| group_order(a, b, chart))
    });
    if tasks.len() > MAX_TASKS {
        return Err(Diagnostic::plain(
            "search_limit",
            "事件數超出計算預算，請縮短片段或調大搜尋取樣間隔".into(),
        ));
    }
    Ok(tasks)
}

/// 同時音的展開順序：接續中的 Slide 先，再依按鍵（Touch 為區號），同按鍵的 Slide 先於 Tap。
/// 先展開的 Slide 讓手接上之後，起點上的 Tap 才能由同一隻手順帶碰到。
fn group_order(a: &Task, b: &Task, chart: &Chart) -> std::cmp::Ordering {
    b.continuation
        .cmp(&a.continuation)
        .then_with(|| chart.notes[a.note].button.cmp(&chart.notes[b.note].button))
        .then_with(|| a.mode.cmp(b.mode))
        .then_with(|| a.note.cmp(&b.note))
}

/// tasks() 依精確的開始時間排序，求解迴圈卻把差在 EPS 內的任務當成同一組。Slide 起點由頭時間
/// 加等待推算，可能比同一時刻的音符晚 1e-14 秒，於是 group_order 失效、Slide 被排到組尾。
/// 回傳整組依 group_order 重排的順序；和現有順序相同時回傳 None。
fn canonical_group(group: &[Task], chart: &Chart) -> Option<Vec<Task>> {
    let mut order = group.to_vec();
    order.sort_by(|a, b| group_order(a, b, chart));
    let same = order
        .iter()
        .zip(group)
        .all(|(a, b)| a.note == b.note && a.mode == b.mode);
    (!same).then_some(order)
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

/// 手在 time 之前一幀內從 point 出發追 Slide。秒數寫法的等待時間會取整，Slide 出發和同一鍵上的
/// Tap 或星星可能差幾十微秒而被拆成兩個時刻；仍在 Tap 的 Critical Perfect（±1 幀）內，同一次按下就算數。
fn pressed_within_frame(arm: &Arm, point: Point, time: f64) -> bool {
    let mut cursor = arm.segments.head.as_ref();
    while let Some(link) = cursor {
        let segment = &link.value;
        if segment.end_seconds < time - JUDGE_FRAME - EPS {
            break;
        }
        let first = &segment.samples[0];
        if segment.mode == "slide"
            && first.time_seconds >= time - JUDGE_FRAME - EPS
            && first.time_seconds <= time + EPS
            && first.point().distance(point) <= EPS
        {
            return true;
        }
        cursor = link.prev.head.as_ref();
    }
    false
}

/// 這條 Slide 在 time 之前要由目前的手繼續追（見 State::committed）。
fn locked(state: &State, note: usize, time: f64) -> bool {
    state
        .committed
        .get(&note)
        .is_some_and(|until| time < until - EPS)
}

/// 被帶著畫的 Slide 在帶它的那條結束前，沒有自己的 checkpoint 接觸。
fn riding(state: &State, task: &Task, chart: &Chart) -> bool {
    state.carried.get(&task.note).is_some_and(|&lead| {
        chart.notes[lead]
            .motion_end
            .is_some_and(|end| task.end <= end + EPS)
    })
}

/// V3：這隻手正準時追著 lead，lead 的星星與這條 Slide 重疊（見 HandRoutes::carriers）。
/// 手順著 lead 畫下去就同時畫過這條，兩條在重疊結束前都由這隻手負責。
fn carry(state: &State, task: &Task, hand: Hand, lead: usize, chart: &Chart) -> Option<State> {
    let n = &chart.notes[task.note];
    let a = &chart.notes[lead];
    // 晚接上時手沿壓縮過的時程走，不在星星上；張開覆蓋兩條時走的是兩顆星星的中點。
    let on_star = state
        .engaged
        .get(&lead)
        .is_some_and(|pickup| (pickup - a.motion_start.unwrap()).abs() < EPS);
    if state.owners.get(&lead) != Some(&hand)
        || !on_star
        || state.finished.contains(&lead)
        || state.bundled.contains_key(&lead)
        || state.bundle_lead.contains_key(&lead)
    {
        return None;
    }
    let start = n.motion_start?;
    let until = a.motion_end?.min(n.motion_end?);
    if task.start >= until - EPS {
        return None;
    }
    let mut next = state.clone();
    next.owners.insert(task.note, hand);
    // 兩顆星星從這條出發起就重合：手經過這條的每個判定區，等同準時接上。
    if next.engaged.insert(task.note, start).is_none() {
        next.pending -= next.deposit.remove(&task.note).unwrap_or(0.0);
    }
    next.carried.insert(task.note, lead);
    let lock = next.committed.entry(lead).or_insert(until);
    *lock = lock.max(until);
    next.used_carry = true;
    next.assignments = next.assignments.push(Assignment {
        note_id: n.id.clone(),
        part: "slide".into(),
        hand,
        start_seconds: start,
        end_seconds: until,
    });
    Some(next)
}

/// 兩手在同一點碰頭時，原地互換各自要追的 Slide。兩手位置相同，交換之後軌跡仍然連續，
/// 因此不需要任何移動 —— 這正是玩家在對穿的 Slide 上避免手臂交叉的作法。
fn swap_at(state: &State, time: f64, chart: &Chart, c: &SolverConfig) -> Option<State> {
    if !c.allow_handover || !state.wifi_pairs.is_empty() {
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
    if c.is_v3()
        && ![first, second].iter().any(|note| {
            has_upcoming_contact(chart, *note, time)
                || state.forced_handovers.contains(&chart.notes[*note].id)
        })
    {
        return None;
    }
    for note in [first, second] {
        let n = &chart.notes[note];
        if time >= n.motion_end? - EPS || locked(state, note, time) {
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
    if task.mode == "slide" && chart.paths[task.path].branches.len() == 2 {
        return if late {
            vec![]
        } else {
            assign_wifi(state, task, hand, chart, c)
        };
    }
    if task.mode == "slide" {
        if let Some(&partner) = state.bundle_lead.get(&task.note) {
            return if late {
                vec![]
            } else {
                assign_bundle(state, task, hand, partner, chart, c)
                    .into_iter()
                    .collect()
            };
        }
    }
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
        let reliefs = relieve(state, idx, hand, task.start, None, false, false, chart, c);
        for &lead in &chart.paths[task.path].hand_routes.carriers {
            out.extend(carry(state, task, hand, lead, chart));
        }
        // 同時出發的另一條 Slide 可由這隻手張開一起覆蓋。
        for (partner, _) in &chart.paths[task.path].hand_routes.bundles {
            if state.engaged.contains_key(partner)
                || state.finished.contains(partner)
                || state.bundled.contains_key(partner)
            {
                continue;
            }
            out.extend(assign_bundle(state, task, hand, *partner, chart, c));
            for relief in &reliefs {
                out.extend(assign_bundle(&relief.state, task, hand, *partner, chart, c));
            }
        }
        reliefs
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

/// V3：一隻手張開同時覆蓋兩條同時的 Slide，沿兩條判定佇列共用的抄近路線移動。
/// 第一次接上時把另一條記為被帶著走，之後只有這隻手能繼續；不做交接。
fn assign_bundle(
    state: &State,
    task: &Task,
    hand: Hand,
    partner: usize,
    chart: &Chart,
    c: &SolverConfig,
) -> Option<State> {
    let n = &chart.notes[task.note];
    let route = &chart.paths[task.path]
        .hand_routes
        .bundles
        .iter()
        .find(|(p, _)| *p == partner)?
        .1;
    if state
        .owners
        .get(&task.note)
        .is_some_and(|owner| *owner != hand)
    {
        return None;
    }
    let engaged = state.engaged.get(&task.note).copied();
    let pickup = engaged.unwrap_or_else(|| task.start.max(state.arms[hand.index()].free));
    if engaged.is_none() && pickup > n.motion_start.unwrap() + c.slide_pickup_seconds + EPS {
        return None;
    }
    let finish = n.motion_end.unwrap();
    let begin = task.start.max(pickup);
    let mut track = task.clone();
    track.samples = route_track(route, task, pickup, finish, begin, c)?;
    let mut next = assign_once(state, &track, hand, chart, c, None)?;
    if engaged.is_none() {
        next.bundle_lead.insert(task.note, partner);
        next.bundled.insert(partner, task.note);
        next.owners.insert(partner, hand);
        if next.engaged.insert(partner, pickup).is_none() {
            next.pending -= next.deposit.remove(&partner).unwrap_or(0.0);
        }
        next.used_bundle = true;
        next.assignments = next.assignments.push(Assignment {
            note_id: chart.notes[partner].id.clone(),
            part: "slide".into(),
            hand,
            start_seconds: begin,
            end_seconds: chart.notes[partner].motion_end.unwrap(),
        });
    }
    Some(next)
}

/// Atomic two-hand WiFi assignment. Both tracks use one pickup clock and keep
/// their split for the whole slide. A circular palm follows the midpoint of
/// two adjacent branches; a single hand must never stand in for all three.
fn assign_wifi(
    state: &State,
    task: &Task,
    paired: Hand,
    chart: &Chart,
    c: &SolverConfig,
) -> Vec<State> {
    let mut bases = vec![state.clone()];
    if !state.engaged.contains_key(&task.note) {
        for hand in [Hand::L, Hand::R] {
            let relieved: Vec<_> = bases
                .iter()
                .flat_map(|base| {
                    relieve(
                        base,
                        hand.index(),
                        hand,
                        task.start,
                        None,
                        false,
                        false,
                        chart,
                        c,
                    )
                    .into_iter()
                    .map(|relief| relief.state)
                })
                .collect();
            bases.extend(relieved);
        }
    }
    bases
        .iter()
        .flat_map(|base| assign_wifi_once(base, task, paired, chart, c))
        .collect()
}

fn assign_wifi_once(
    state: &State,
    task: &Task,
    paired: Hand,
    chart: &Chart,
    c: &SolverConfig,
) -> Vec<State> {
    if state
        .owners
        .get(&task.note)
        .is_some_and(|owner| *owner != paired)
    {
        return vec![];
    }
    let routes = &chart.paths[task.path].hand_routes;
    if c.is_v3() && (routes.wifi_all.is_some() || routes.wifi_pair.iter().any(Option::is_some)) {
        return assign_wifi_routes(state, task, paired, chart, c);
    }
    let note = &chart.notes[task.note];
    let path = &chart.paths[task.path];
    let nominal_end = note.motion_end.unwrap();
    let pickup = state
        .engaged
        .get(&task.note)
        .copied()
        .unwrap_or_else(|| task.start.max(state.arms[0].free).max(state.arms[1].free));
    if pickup > note.motion_start.unwrap() + c.slide_pickup_seconds + EPS {
        return vec![];
    }
    let begin = task.start.max(pickup);
    let end = if task.release_early {
        task.end - c.slide_pickup_seconds.min((task.end - task.start) * 0.5)
    } else {
        task.end
    };
    if begin >= end - EPS {
        return vec![];
    }
    let u0 = (begin - pickup) / (nominal_end - pickup);
    let u1 = if task.release_early {
        1.0
    } else {
        (end - pickup) / (nominal_end - pickup)
    };
    let other = if paired == Hand::L { Hand::R } else { Hand::L };
    let mut out = vec![];
    for side in 0..2 {
        if state.wifi_pairs.get(&task.note).is_some_and(|s| *s != side) {
            continue;
        }
        let side_end = |s: usize| {
            let p = path.branches[s].last().unwrap();
            Point { x: p.x, y: p.y }
        };
        let center_end = path.at(1.0);
        // Maximum separation occurs at the fan's endpoint.
        if center_end.distance(side_end(side)) * 0.5 > c.palm_radius + EPS {
            continue;
        }
        let ends = [center_end.lerp(side_end(side), 0.5), side_end(1 - side)];
        let mut next = state.clone();
        next.pending -= next.deposit.remove(&task.note).unwrap_or(0.0);
        next.engaged.insert(task.note, pickup);
        let mut valid = true;
        for (hand, endpoint) in [paired, other].into_iter().zip(ends) {
            next.owners.insert(task.note, hand);
            let mut track = task.clone();
            track.samples = vec![
                MotionSample::new(begin, path.at(0.0).lerp(endpoint, u0)),
                MotionSample::new(end, path.at(0.0).lerp(endpoint, u1)),
            ];
            if let Some(assigned) = assign_once(&next, &track, hand, chart, c, None) {
                next = assigned;
            } else {
                valid = false;
                break;
            }
        }
        if valid {
            next.owners.insert(task.note, paired);
            next.wifi_pairs.insert(task.note, side);
            out.push(next);
        }
    }
    out
}

/// V3 WiFi：依三條判定佇列的抄近路線分工。side 0／1 為中央＋該側由 paired 覆蓋、
/// 另一側由另一隻手；side 2 為 paired 單手張開覆蓋三條，另一隻手空出來。
/// 真人基本上用雙手 2+1，單手張開只動一隻手、成本總是比較低，所以只當備用：
/// 2+1 做不到（另一隻手抽不開）、另一隻手在 WiFi 期間要接別的音符，或真人標註指定單手時才展開。
fn assign_wifi_routes(
    state: &State,
    task: &Task,
    paired: Hand,
    chart: &Chart,
    c: &SolverConfig,
) -> Vec<State> {
    let note = &chart.notes[task.note];
    let routes = &chart.paths[task.path].hand_routes;
    let nominal_end = note.motion_end.unwrap();
    let other = if paired == Hand::L { Hand::R } else { Hand::L };
    let mut out = vec![];
    for side in 0..3 {
        if state.wifi_pairs.get(&task.note).is_some_and(|s| *s != side) {
            continue;
        }
        if side == 2
            && !out.is_empty()
            && !other_hand_needed(chart, task.note, task.start)
            && !state.single_track.contains(&note.id)
        {
            continue;
        }
        let tracks: Vec<(Hand, &Vec<PathSample>)> = if side < 2 {
            match (&routes.wifi_pair[side], &routes.wifi_single[1 - side]) {
                (Some(pair), Some(single)) => vec![(paired, pair), (other, single)],
                _ => continue,
            }
        } else {
            match &routes.wifi_all {
                Some(all) => vec![(paired, all)],
                None => continue,
            }
        };
        let pickup = state.engaged.get(&task.note).copied().unwrap_or_else(|| {
            tracks.iter().fold(task.start, |t, (hand, _)| {
                t.max(state.arms[hand.index()].free)
            })
        });
        if pickup > note.motion_start.unwrap() + c.slide_pickup_seconds + EPS {
            continue;
        }
        let begin = task.start.max(pickup);
        let mut next = state.clone();
        next.pending -= next.deposit.remove(&task.note).unwrap_or(0.0);
        next.engaged.insert(task.note, pickup);
        let mut valid = true;
        for (hand, route) in tracks {
            next.owners.insert(task.note, hand);
            let Some(samples) = route_track(route, task, pickup, nominal_end, begin, c) else {
                valid = false;
                break;
            };
            let mut track = task.clone();
            track.samples = samples;
            match assign_once(&next, &track, hand, chart, c, None) {
                Some(assigned) => next = assigned,
                None => {
                    valid = false;
                    break;
                }
            }
        }
        if valid {
            next.owners.insert(task.note, paired);
            next.wifi_pairs.insert(task.note, side);
            out.push(next);
        }
    }
    out
}

/// WiFi 滑行期間（含開始的同一刻）有沒有別的音符要接觸；有的話單手張開才有意義。
/// 開始那一刻落在起點鍵上的 Tap 由起手那隻手順手按掉，不算。
fn other_hand_needed(chart: &Chart, note: usize, start: f64) -> bool {
    let wifi = &chart.notes[note];
    let end = wifi.motion_end.unwrap();
    let during = |t: f64| t > start - EPS && t < end - EPS;
    chart.notes.iter().enumerate().any(|(i, other)| {
        let at_start_button = other.touch_area.is_none()
            && other.button == wifi.button
            && (other.time_seconds - start).abs() < EPS;
        i != note
            && ((other.has_head && during(other.time_seconds) && !at_start_button)
                || other.motion_start.is_some_and(during))
    })
}

fn has_upcoming_contact(chart: &Chart, note: usize, time: f64) -> bool {
    let end = chart.notes[note].motion_end.unwrap();
    chart.notes.iter().enumerate().any(|(i, other)| {
        i != note
            && ((other.has_head
                && other.time_seconds > time + EPS
                && other.time_seconds < end - EPS)
                || other
                    .motion_start
                    .is_some_and(|t| t > time + EPS && t < end - EPS))
    })
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
    let mut samples = if task.mode == "slide" && !task.samples.is_empty() {
        task.samples.clone()
    } else {
        match (pickup, early_finish) {
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
        }
    };
    // 被帶著畫的 Slide 在帶它的那條結束時接回自己的路徑：兩顆星星只差取樣誤差，從手的位置接下去。
    if task.mode == "slide"
        && state.carried.contains_key(&task.note)
        && state.owners.get(&task.note) == Some(&hand)
        && (state.arms[idx].free - begin).abs() <= EPS
        && state.arms[idx].point.distance(samples[0].point()) <= CARRY_TOLERANCE
    {
        samples[0] = MotionSample::new(begin, state.arms[idx].point);
    }
    let start = samples[0].point();

    if state.arms[idx].free > begin + EPS {
        // 手上已有任務，但接觸點就在同一個位置：同一隻手一次接觸即可滿足兩者
        // （例如 Slide 起點上的 Break Tap）。不另外產生軌跡段，也不另外計成本。
        if task.mode == "tap"
            && (arm_point_at(&state.arms[idx], task.start).distance(start) <= EPS
                || pressed_within_frame(&state.arms[idx], start, task.start))
        {
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
        if locked(state, task.note, task.start) {
            return None;
        }
        // V3 keeps ownership unless another contact is due before this slide ends.
        // Look ahead so the hand can overlap and travel before the actual conflict.
        if c.is_v3()
            && !has_upcoming_contact(chart, task.note, task.start)
            && !state.forced_handovers.contains(&n.id)
        {
            return None;
        }
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
    if task.mode != "slide" {
        record_action(
            &mut next,
            c,
            hand,
            start,
            begin,
            if glide || glided {
                ActionKind::ContinuousContact
            } else {
                ActionKind::Strike
            },
            Some(n.id.clone()),
        );
    }
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
    record_action(
        &mut next,
        c,
        hand,
        center,
        start,
        // A palm reached without lifting is still continuous contact, not a
        // fresh placement. Charging a full placement biases dense wipes late.
        if glided {
            ActionKind::ContinuousContact
        } else {
            ActionKind::Palm
        },
        note_ids.first().cloned(),
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

/// 同一組裡相鄰兩鍵的 Tap（含星星起點）配對，回傳組內索引。
fn adjacent_pairs(group: &[Task], chart: &Chart) -> Vec<[usize; 2]> {
    let button = |task: &Task| {
        let n = &chart.notes[task.note];
        (task.mode == "tap" && matches!(n.kind.as_str(), "tap" | "slide")).then_some(n.button)
    };
    let mut out = vec![];
    for (i, a) in group.iter().enumerate() {
        let Some(x) = button(a) else {
            continue;
        };
        for (j, b) in group.iter().enumerate().skip(i + 1) {
            if button(b).is_some_and(|y| matches!((x + 8 - y) % 8, 1 | 7)) {
                out.push([i, j]);
            }
        }
    }
    out
}

/// 一隻手張開同時按下相鄰兩鍵（相距約 0.77 個盤面半徑；隔一鍵以上就搆不到）：手落在兩鍵
/// 中點，兩顆都算這隻手。真人很少這樣按（標記裡約千分之一），所以只在標記者這樣標，或整首
/// 最後的放寬重試裡這一組兩手怎麼分都走不下去時才展開。
fn pair_press(
    state: &State,
    pair: [usize; 2],
    group: &[Task],
    hand: Hand,
    chart: &Chart,
    c: &SolverConfig,
) -> Vec<State> {
    let time = group[pair[0]].start;
    let [a, b] = pair.map(|i| chart.notes[group[i].note].position);
    let center = a.lerp(b, 0.5);
    let mut out: Vec<State> = pair_once(state, pair, center, group, hand, chart, c, time, false)
        .into_iter()
        .collect();
    for relief in relieve(
        state,
        hand.index(),
        hand,
        time,
        Some(center),
        false,
        false,
        chart,
        c,
    ) {
        out.extend(pair_once(
            &relief.state,
            pair,
            center,
            group,
            hand,
            chart,
            c,
            relief.begin,
            relief.glided,
        ));
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn pair_once(
    state: &State,
    pair: [usize; 2],
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
    if arm.free > start + EPS || (start - arm.free <= EPS && arm.point.distance(center) > EPS) {
        return None;
    }
    let end = pair.iter().map(|i| group[*i].end).fold(start, f64::max);
    let first = chart.notes[group[pair[0]].note].id.clone();
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
            * (1.0 - (start - last) / c.repetition_seconds)
                .max(0.0)
                .powi(2);
    }
    a.last_tap = Some(start);
    add_segment(
        a,
        hand,
        vec![
            MotionSample::new(start, center),
            MotionSample::new(end, center),
        ],
        "tap",
        Some(first.clone()),
        &mut next.cost,
        c,
    );
    record_action(
        &mut next,
        c,
        hand,
        center,
        start,
        if glided {
            ActionKind::ContinuousContact
        } else {
            ActionKind::Palm
        },
        Some(first),
    );
    for i in pair {
        let n = &chart.notes[group[i].note];
        next.assignments = next.assignments.push(Assignment {
            note_id: n.id.clone(),
            part: if n.kind == "slide" { "head" } else { "contact" }.into(),
            hand,
            start_seconds: start,
            end_seconds: group[i].end,
        });
    }
    next.used_pair_press = true;
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

/// 這隻手正在追、之後會沿路徑畫下去的 Slide，以及手從目前排定的終點畫到 until 的位置取樣。
/// 張開覆蓋兩條的路線與 WiFi 不在這裡預看（手走的不是這條的星星）。
fn ahead_track(
    state: &State,
    hand: Hand,
    chart: &Chart,
    until: f64,
) -> Option<(usize, Vec<MotionSample>)> {
    let arm = &state.arms[hand.index()];
    let last = arm.segments.last()?;
    if last.mode != "slide" {
        return None;
    }
    let (&note, _) = state
        .owners
        .iter()
        .find(|(n, h)| **h == hand && Some(&chart.notes[**n].id) == last.note_id.as_ref())?;
    let n = &chart.notes[note];
    let path = chart
        .paths
        .iter()
        .position(|p| Some(&p.id) == n.path_id.as_ref())?;
    if chart.paths[path].branches.len() == 2
        || state.bundle_lead.contains_key(&note)
        || state.bundled.contains_key(&note)
        || state.finished.contains(&note)
    {
        return None;
    }
    let pickup = *state.engaged.get(&note)?;
    let end = n.motion_end?;
    let to = end.min(until);
    if to <= arm.free + EPS {
        return None;
    }
    Some((
        note,
        path_samples(&chart.paths[path], pickup, end, arm.free, to),
    ))
}

/// Touch 沒有 Fast 判定，且只看感應區是否被按著。手若在 Critical Perfect 區間內
/// 貼著面板經過落點附近（追 Slide、滑移、按住 Hold 或停留的 Tap），這個 Touch
/// 就順帶完成，不另外產生動作段。除了已經排定的動作，V3 也預看追 Slide 的手接下來的路徑：
/// 譜面常把 Touch 放在 Slide 路徑上，讓滑過去的手一起碰掉。
fn brush_touch(
    state: &State,
    task: &Task,
    hand: Hand,
    chart: &Chart,
    c: &SolverConfig,
) -> Option<State> {
    let note = &chart.notes[task.note];
    // V3：Tap 也能用螢幕外圈 A 區觸發。追著 Slide 的手在 Tap 判定時刻（±1 幀）手掌正好
    // 蓋得到該鍵的 A 區時，可順手點下去，不另外派手。
    let tap = c.is_v3() && note.kind == "tap";
    if task.mode != "tap" || !(tap || note.kind == "touch" && c.palm_radius > 0.0) {
        return None;
    }
    let window = if tap {
        JUDGE_FRAME
    } else {
        TOUCH_LATE_FRAMES * JUDGE_FRAME
    };
    let (lo, hi) = (task.start - window, task.start + window);
    let target = if tap {
        crate::judge::sensor_point(&format!("A{}", note.button))
    } else {
        note.position
    };
    // 外圈按鍵在螢幕外；按住或滑過按鍵的手不會碰到螢幕上的感應區。
    let on_screen = |p: Point| p.x.hypot(p.y) < SCREEN_EDGE;
    // (距離, 時刻, 靠預看才碰到時的那條 Slide)
    let mut best: Option<(f64, f64, Option<usize>)> = None;
    let mut consider = |samples: &[MotionSample], tracking: bool, ahead: Option<usize>| {
        // 追 Slide 的手是張開貼在螢幕上的：手掌範圍內的 A 區都碰得到（至少指尖可及）。
        let radius = if tap {
            crate::judge::finger_reach('A').max(c.palm_radius)
        } else if tracking {
            c.palm_radius
        } else {
            TOUCH_BRUSH_RADIUS.min(c.palm_radius)
        };
        for pair in samples.windows(2) {
            let (t0, t1) = (pair[0].time_seconds, pair[1].time_seconds);
            let (a, b) = (pair[0].point(), pair[1].point());
            if !(tracking || on_screen(a) && on_screen(b)) {
                continue;
            }
            let (from, to) = (t0.max(lo), t1.min(hi));
            if to < from {
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
            let (enter, exit) = if length <= EPS {
                // 幾乎沒有移動：頭或尾任一端碰得到即可，取碰得到的那一端。
                if p.distance(target) <= radius + EPS {
                    (from, to)
                } else if q.distance(target) <= radius + EPS {
                    (to, to)
                } else {
                    continue;
                }
            } else {
                let projection = ((target.x - p.x) * dx + (target.y - p.y) * dy) / length;
                let closest = Point {
                    x: p.x + projection * dx,
                    y: p.y + projection * dy,
                };
                let perpendicular = closest.distance(target);
                if perpendicular > radius + EPS {
                    continue;
                }
                let half =
                    ((radius * radius - perpendicular * perpendicular).max(0.0) / length).sqrt();
                let (low, high) = ((projection - half).max(0.0), (projection + half).min(1.0));
                if low > high {
                    continue;
                }
                (from + (to - from) * low, from + (to - from) * high)
            };
            // Prefer the nominal Touch time when already inside the moving palm,
            // rather than delaying to the geometric closest point/checkpoint.
            let time = task.start.clamp(enter, exit);
            let distance = at(time).distance(target);
            // 壓在實體按鍵上（螢幕外）是一般的敲擊，不算用 A 區順手觸發。
            if tap && !on_screen(at(time)) {
                continue;
            }
            if distance <= radius + EPS
                && best.is_none_or(|(d, t, _)| {
                    (time - task.start).abs() < (t - task.start).abs() - EPS
                        || ((time - task.start).abs() - (t - task.start).abs()).abs() <= EPS
                            && distance < d - EPS
                })
            {
                best = Some((distance, time, ahead));
            }
        }
    };
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
        let tracking = matches!(segment.mode.as_str(), "slide" | "handover");
        if tap && !tracking {
            continue;
        }
        consider(&segment.samples, tracking, None);
    }
    // 預看只用在 Touch：接下來要畫的路徑在判定區間內經過的話，這條 Slide 就得追到那一刻。
    if !tap && c.is_v3() {
        if let Some((slide, samples)) = ahead_track(state, hand, chart, hi) {
            consider(&samples, true, Some(slide));
        }
    }
    let (_, time, ahead) = best?;
    let mut next = state.clone();
    if let Some(slide) = ahead {
        let lock = next.committed.entry(slide).or_insert(time);
        *lock = lock.max(time);
    }
    next.assignments = next.assignments.push(Assignment {
        note_id: note.id.clone(),
        part: "contact".into(),
        hand,
        start_seconds: time,
        end_seconds: time + JUDGE_FRAME,
    });
    if tap {
        // Tap 是按鍵接觸：手已貼在螢幕上，記為不抬手的連續接觸，V3 的分工與負荷照常計入。
        record_action(
            &mut next,
            c,
            hand,
            button(note.button),
            task.start,
            ActionKind::ContinuousContact,
            Some(note.id.clone()),
        );
    }
    Some(next)
}

/// 暫停中的 Slide 由 hand 在 task.start 接回去：從暫停時的進度 progress 追到名目終點。
/// 以「虛擬接上時間」表示：沿用晚接上時剩餘路徑等速壓縮的公式，
/// u(t) = (t - pickup) / (end - pickup) 在 task.start 時剛好等於 progress。
fn resume_slide(
    state: &State,
    task: &Task,
    hand: Hand,
    progress: f64,
    chart: &Chart,
) -> Option<State> {
    let end = chart.notes[task.note].motion_end?;
    if task.start >= end - EPS || progress >= 1.0 - EPS {
        return None;
    }
    if task.path_length * (1.0 - progress) > RESUME_MAX_SPEED * (end - task.start) {
        return None;
    }
    let pickup = (task.start - progress * end) / (1.0 - progress);
    let mut next = state.clone();
    next.suspended.remove(&task.note);
    next.resumed.insert(task.note);
    next.engaged.insert(task.note, pickup);
    next.owners.insert(task.note, hand);
    next.pending -= next.deposit.remove(&task.note).unwrap_or(0.0);
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

/// 這個狀態在 time 那一組略過了幾個音符部分（略過依時間先後記，這一組的在最前面）。
fn drops_at(state: &State, time: f64) -> usize {
    let mut count = 0;
    let mut cursor = state.dropped.head.as_ref();
    while let Some(link) = cursor {
        if (link.value.time - time).abs() > EPS {
            break;
        }
        count += 1;
        cursor = link.prev.head.as_ref();
    }
    count
}

/// 這顆音符的接觸是不是在 time 那一組略過了。
fn dropped_at(state: &State, note: usize, time: f64) -> bool {
    let mut cursor = state.dropped.head.as_ref();
    while let Some(link) = cursor {
        if (link.value.time - time).abs() > EPS {
            return false;
        }
        if link.value.note == note && !link.value.slide {
            return true;
        }
        cursor = link.prev.head.as_ref();
    }
    false
}

/// 部分解：略過這個任務的音符部分。接觸就是不指派；Slide 當成已完成、放開手（同提早離手），
/// 之後的 checkpoint 不再需要手。帶著別條一起畫（重疊星星、張開覆蓋）的 Slide 放掉時，
/// 被帶著的那條也沒有手畫，一起略過。
fn drop_part(state: &State, task: &Task, time: f64) -> State {
    let mut s = state.clone();
    let slide = task.mode == "slide";
    let mut notes = vec![task.note];
    if slide {
        notes.extend(
            s.carried
                .iter()
                .filter(|(_, lead)| **lead == task.note)
                .map(|(note, _)| *note),
        );
        notes.extend(s.bundle_lead.get(&task.note).copied());
    }
    for note in notes {
        if slide {
            s.finished.insert(note);
            s.owners.remove(&note);
            s.engaged.remove(&note);
            s.suspended.remove(&note);
            s.wifi_pairs.remove(&note);
            s.committed.remove(&note);
            s.carried.remove(&note);
            s.bundled.remove(&note);
            s.bundle_lead.remove(&note);
            if let Some(held) = s.deposit.remove(&note) {
                s.pending -= held;
            }
        }
        s.dropped = s.dropped.push(Dropped { note, slide, time });
    }
    s
}

/// 整組指派完成時驗證 Touch Group：實際接觸必須超過半數，其餘才可連帶判定。
/// 連帶判定的 Touch 以最後一個實際接觸者的手記錄，part 為 "group"，不產生動作。
/// 求解帶著手部限制（標註或 Temoto 的修正）時，連帶判定的 Touch 照限制的手記錄：
/// 它沒有實際接觸，記哪隻手只是標示，不該因此判成限制走不通。
fn settle_touch_groups(
    mut state: State,
    auto: &[bool],
    touch_groups: &[Vec<usize>],
    group: &[Task],
    chart: &Chart,
    rules: Option<&HandRules>,
) -> Option<State> {
    if !auto.iter().any(|a| *a) {
        return Some(state);
    }
    for members in touch_groups {
        if !members.iter().any(|i| auto[*i]) {
            continue;
        }
        // 部分解略過的 Touch 沒有接觸，不算進過半。
        let contacted: Vec<usize> = members
            .iter()
            .copied()
            .filter(|i| !auto[*i] && !dropped_at(&state, group[*i].note, group[*i].start))
            .collect();
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
            let hand = rules
                .and_then(|r| r.contact.get(&note.id).copied())
                .unwrap_or(hand);
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

/// 兩手拍下一組 Touch 時，一隻手在這一刻做的事。
enum SlapAction {
    Idle,
    Touch(usize),
    Palm(PalmCandidate),
}

/// 整組都是 Touch（Tap）時，直接列舉兩隻手各自的動作（不動、單點或一個掌面）：沒蓋到的
/// Touch 先看手有沒有順帶擦到，其餘交給 Touch Group 過半判定。SLAP_FIRST_NOTES 顆以上的
/// 大組直接用；較小的組照舊逐顆展開，走不下去時才用這個。
///
/// 逐顆展開時，手接得到的 Touch 一定要接，而且照固定順序指派，兩隻手常被排在前面的幾顆
/// （例如中央）佔走，「兩掌各蓋外圈一團、中間交給過半判定」這類接法展不出來，整組就無解；
/// 分支又隨顆數指數成長。這裡先用涵蓋的音符（位元遮罩）篩掉過不了半的組合才建狀態，
/// 所以 Touch 很多也不會爆量。
///
/// `centered`：每個掌面只放在蓋住那幾顆的正中間（最靠近它們重心的候選掌心），不再逐一試
/// 每個掌心。大組用這個：同一團換個掌心差別很小，候選卻會多出好幾倍。
///
/// 帶手部限制（標註、TemotoAI 的修正）時只回傳符合限制的狀態（擱著的標記 `aside` 不算）；
/// 直接碰到的音符被指定給另一隻手的動作，連狀態都不建。「兩手都動找不到才讓一隻手不動」
/// 也是看符合限制的結果。
#[allow(clippy::too_many_arguments)]
fn touch_slap(
    beam: &[State],
    group: &[Task],
    chart: &Chart,
    c: &SolverConfig,
    relaxed: bool,
    centered: bool,
    candidates: &[PalmCandidate],
    touch_groups: &[Vec<usize>],
    rules: Option<&HandRules>,
    aside: &BTreeSet<String>,
    v2_context: Option<&v2::Context>,
    v3_context: Option<&v3::Context>,
) -> Result<Vec<State>, Diagnostic> {
    let n = group.len();
    if n > 64 {
        return Ok(vec![]);
    }
    let bits = |members: &[usize]| members.iter().fold(0u64, |m, i| m | 1 << i);
    let full = if n == 64 { u64::MAX } else { (1u64 << n) - 1 };
    let grouped: Vec<u64> = touch_groups.iter().map(|g| bits(g)).collect();
    let loose = grouped.iter().fold(full, |m, g| m & !g);
    // 不在 Touch Group 裡的每顆都要碰到；每個 Touch Group 要碰到過半。
    let enough = |hit: u64| {
        hit & loose == loose
            && grouped
                .iter()
                .all(|g| 2 * (hit & g).count_ones() > g.count_ones())
    };
    let mut actions: Vec<(u64, SlapAction)> = vec![(0, SlapAction::Idle)];
    actions.extend((0..n).map(|i| (1u64 << i, SlapAction::Touch(i))));
    for candidate in candidates {
        let mask = bits(&candidate.covered);
        if c.is_legacy() {
            actions.push((
                mask,
                SlapAction::Palm(PalmCandidate {
                    covered: candidate.covered.clone(),
                    centers: candidate.centers.clone(),
                }),
            ));
        } else if centered {
            let count = candidate.covered.len() as f64;
            let (x, y) = candidate.covered.iter().fold((0.0, 0.0), |(x, y), i| {
                let p = chart.notes[group[*i].note].position;
                (x + p.x, y + p.y)
            });
            let middle = Point {
                x: x / count,
                y: y / count,
            };
            if let Some(center) = candidate
                .centers
                .iter()
                .copied()
                .min_by(|a, b| a.distance(middle).total_cmp(&b.distance(middle)))
            {
                actions.push((
                    mask,
                    SlapAction::Palm(PalmCandidate {
                        covered: candidate.covered.clone(),
                        centers: vec![center],
                    }),
                ));
            }
        } else {
            actions.extend(candidate.centers.iter().map(|center| {
                (
                    mask,
                    SlapAction::Palm(PalmCandidate {
                        covered: candidate.covered.clone(),
                        centers: vec![*center],
                    }),
                )
            }));
        }
    }
    // 直接碰到的音符被限制給另一隻手：這個動作由這隻手做，建出來也會被擋掉。
    // AI 重解時同一組大和弦每一輪都要再列舉一次，先排除可以省掉大部分的狀態。
    let conflicts = |action: &SlapAction, hand: Hand| {
        let Some(r) = rules else {
            return false;
        };
        let touched: &[usize] = match action {
            SlapAction::Idle => &[],
            SlapAction::Touch(i) => std::slice::from_ref(i),
            SlapAction::Palm(palm) => &palm.covered,
        };
        touched.iter().any(|&i| {
            let id = &chart.notes[group[i].note].id;
            !aside.contains(id) && r.contact.get(id).is_some_and(|h| *h != hand)
        })
    };
    let usable: Vec<[bool; 2]> = actions
        .iter()
        .map(|(_, a)| [!conflicts(a, Hand::L), !conflicts(a, Hand::R)])
        .collect();
    let update = |before: &State, after: &mut State| -> Result<(), Diagnostic> {
        if let Some(context) = v2_context {
            context.update(before, after)?;
        }
        if let Some(context) = v3_context {
            context.update(before, after)?;
        }
        Ok(())
    };
    let act = |state: &State,
               action: &SlapAction,
               hand: Hand,
               late: bool|
     -> Result<Vec<State>, Diagnostic> {
        let mut out = match action {
            SlapAction::Idle => return Ok(vec![state.clone()]),
            SlapAction::Touch(i) => assign(state, &group[*i], hand, chart, c, late),
            SlapAction::Palm(palm) => assign_palm(state, palm, group, hand, chart, c, late),
        };
        for s in &mut out {
            s.lenient_in_group |= late;
            update(state, s)?;
        }
        Ok(out)
    };
    let lates: &[bool] = if relaxed { &[false, true] } else { &[false] };
    let idle = |action: &SlapAction| matches!(action, SlapAction::Idle);
    let mut out = vec![];
    for state in beam {
        let before = out.len();
        // 左手動作的結果和右手做什麼無關，同一個狀態只算一次。
        let mut lefts: Vec<[Option<Vec<State>>; 2]> =
            (0..actions.len()).map(|_| [None, None]).collect();
        // 手空著就要接（同逐顆展開）：先只試兩手都有動作的組合，都不行才讓一隻手不動。
        for with_idle in [false, true] {
            if with_idle && out.len() > before {
                break;
            }
            for (li, (left_mask, left)) in actions.iter().enumerate() {
                for (ri, (right_mask, right)) in actions.iter().enumerate() {
                    let hit = left_mask | right_mask;
                    if (idle(left) || idle(right)) != with_idle
                        || left_mask & right_mask != 0
                        || !usable[li][0]
                        || !usable[ri][1]
                        || !enough(hit)
                    {
                        continue;
                    }
                    for &late in lates {
                        let slot = &mut lefts[li][usize::from(late)];
                        if slot.is_none() {
                            *slot = Some(act(state, left, Hand::L, late)?);
                        }
                        for after_left in slot.as_deref().unwrap_or_default() {
                            for mut s in act(after_left, right, Hand::R, late)? {
                                if cancelled() {
                                    return Err(Diagnostic::plain(
                                        "cancelled",
                                        "分析已取消".into(),
                                    ));
                                }
                                let mut auto = vec![false; n];
                                for (i, task) in group.iter().enumerate() {
                                    if hit & 1 << i != 0 {
                                        continue;
                                    }
                                    let brushed = brush_touch(&s, task, Hand::L, chart, c)
                                        .or_else(|| brush_touch(&s, task, Hand::R, chart, c));
                                    match brushed {
                                        Some(mut next) => {
                                            update(&s, &mut next)?;
                                            s = next;
                                        }
                                        None => auto[i] = true,
                                    }
                                }
                                let Some(mut s) = settle_touch_groups(
                                    s,
                                    &auto,
                                    touch_groups,
                                    group,
                                    chart,
                                    rules,
                                ) else {
                                    continue;
                                };
                                if let Some(context) = v2_context {
                                    context.refresh(&mut s)?;
                                }
                                if let Some(context) = v3_context {
                                    context.refresh(&mut s)?;
                                }
                                if rules.is_some_and(|r| !r.violations(&s, aside).is_empty()) {
                                    continue;
                                }
                                out.push(s);
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(out)
}

/// 這一組同時音的錯誤：標出時間、音符與原文位置，使用者才知道卡在哪裡。
fn group_diagnostic(
    code: &str,
    message: String,
    time: f64,
    group: &[Task],
    chart: &Chart,
) -> Diagnostic {
    let mut d = Diagnostic::plain(code, message);
    d.time_seconds = Some(time);
    d.note_ids = group
        .iter()
        .map(|t| chart.notes[t.note].id.clone())
        .collect();
    d.note_ids.sort();
    d.note_ids.dedup();
    d.source_span = Some(Box::new(chart.notes[group[0].note].source_span.clone()));
    d
}

/// 同時音的候選太多、算不完時的說明。調小搜尋寬度，每一組要展開的狀態就變少。
const SEARCH_LIMIT_MESSAGE: &str = "同時事件的候選已達計算預算，請調小搜尋寬度或縮短片段";

/// 整組都是 Touch、至少這麼多顆時先用 touch_slap：逐顆展開 8 顆起一組就要零點幾秒，
/// 13 顆要近兩秒（搜尋寬度 128），而且常因固定順序找不到接法。
const SLAP_FIRST_NOTES: usize = 8;

fn is_touch_sweep_group(group: &[Task], chart: &Chart) -> bool {
    group.len() >= MIN_TOUCH_SWEEP_NOTES
        && group
            .iter()
            .all(|task| task.mode == "tap" && chart.notes[task.note].kind.as_str() == "touch")
}

/// 半邊盤面的 Touch 由上往下刷（玩家的習慣）：y 小的先，同高度時外側先，再以音符
/// 索引固定順序，讓輸出在不同執行間保持一致。
fn order_touch_sweep_route(mut route: Vec<usize>, group: &[Task], chart: &Chart) -> Vec<usize> {
    let key = |task: usize| {
        let p = chart.notes[group[task].note].position;
        (
            (p.y * 1e6).round() as i64,
            -(p.x.abs() * 1e6).round() as i64,
            group[task].note,
        )
    };
    route.sort_by_key(|task| key(*task));
    route
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
    let mut middle = vec![];
    for (task_index, task) in group.iter().enumerate() {
        let x = chart.notes[task.note].position.x;
        if x < -EPS {
            routes[usize::from(reversed)].push(task_index);
        } else if x > EPS {
            routes[1 - usize::from(reversed)].push(task_index);
        } else {
            middle.push(task_index);
        }
    }
    // 中線上的落點在兩邊分好之後，輪流交給目前比較少的那隻手。
    for task_index in middle {
        let side = usize::from(routes[0].len() > routes[1].len());
        routes[side].push(task_index);
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

        let route = order_touch_sweep_route(std::mem::take(&mut routes[idx]), group, chart);
        let from = next.arms[idx].point;
        let window = (route_start, judgment);
        let (mut samples, hits) = palm_stripe(&route, group, chart, from, window, c.palm_radius)
            .unwrap_or_else(|| point_route(&route, group, chart, from, window));
        let finish = hits
            .iter()
            .map(|(task_index, _, _)| group[*task_index].end)
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
        // 同一刻碰到的一排（手掌刷過同一個高度）只算一個動作，每顆仍各有接觸紀錄。
        let mut row = f64::NEG_INFINITY;
        for (task_index, hit, point) in hits {
            let note = &chart.notes[group[task_index].note];
            if hit > row + EPS {
                row = hit;
                record_action(
                    &mut next,
                    c,
                    hand,
                    point,
                    hit,
                    ActionKind::ContinuousContact,
                    Some(note.id.clone()),
                );
            }
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

/// 掃屏一隻手的軌跡，與每顆 Touch 的接觸時刻和那時的手位。
type SweepPlan = (Vec<MotionSample>, Vec<(usize, f64, Point)>);

/// 手掌沿半邊的中線由上往下直刷：掌心 x 取這隻手落點左右範圍的中點，刷到哪個高度
/// 就碰到那個高度的 Touch。先從目前手位移到最上面，所花時間依距離分配，另加四分之一
/// 刷的長度，手已經在上面時也不會瞬間開始。掌心到任一落點超過手掌半徑（例如手掌
/// 半徑調小）時回傳 None，改逐顆點過去。`route` 已由上往下排好。
fn palm_stripe(
    route: &[usize],
    group: &[Task],
    chart: &Chart,
    from: Point,
    (start, end): (f64, f64),
    radius: f64,
) -> Option<SweepPlan> {
    let points: Vec<Point> = route
        .iter()
        .map(|task| chart.notes[group[*task].note].position)
        .collect();
    let (lo, hi) = points
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(p.x), hi.max(p.x))
        });
    let x = (lo + hi) / 2.0;
    let limit = (1.0 - x * x).max(0.0).sqrt();
    let at = |y: f64| Point {
        x,
        y: y.clamp(-limit, limit),
    };
    if points.iter().any(|p| at(p.y).distance(*p) > radius + EPS) {
        return None;
    }
    let (top, bottom) = (at(points[0].y), at(points[points.len() - 1].y));
    let sweep = bottom.y - top.y;
    if sweep <= EPS {
        return None;
    }
    let lead = from.distance(top) + sweep / 4.0;
    let reach = start + (end - start) * lead / (lead + sweep);
    let hits = route
        .iter()
        .zip(&points)
        .map(|(task, p)| {
            let share = (at(p.y).y - top.y) / sweep;
            let hit = if share >= 1.0 - EPS {
                end
            } else {
                reach + (end - reach) * share
            };
            (*task, hit, at(p.y))
        })
        .collect();
    let samples = vec![
        MotionSample::new(start, from),
        MotionSample::new(reach, top),
        MotionSample::new(end, bottom),
    ];
    Some((samples, hits))
}

/// 手掌蓋不住半邊寬度時，手逐顆點過去（仍由上往下）。每一步的時間依「移動距離＋
/// 平均步長」分配：遠的步不會瞬間跳過去，同位置的兩顆也不會擠在同一刻。
fn point_route(
    route: &[usize],
    group: &[Task],
    chart: &Chart,
    from: Point,
    (start, end): (f64, f64),
) -> SweepPlan {
    let mut point = from;
    let legs: Vec<f64> = route
        .iter()
        .map(|task| {
            let next = chart.notes[group[*task].note].position;
            let leg = point.distance(next);
            point = next;
            leg
        })
        .collect();
    let unit = (legs.iter().sum::<f64>() / legs.len() as f64).max(EPS);
    let total: f64 = legs.iter().map(|leg| leg + unit).sum();
    let mut samples = vec![MotionSample::new(start, from)];
    let mut hits = Vec::with_capacity(route.len());
    let mut done = 0.0;
    for (step, (task, leg)) in route.iter().zip(&legs).enumerate() {
        done += leg + unit;
        let share = if step + 1 == route.len() {
            1.0
        } else {
            (done / total).min(1.0)
        };
        let hit = start + (end - start) * share;
        let point = chart.notes[group[*task].note].position;
        samples.push(MotionSample::new(hit, point));
        hits.push((*task, hit, point));
    }
    (samples, hits)
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

thread_local! {
    /// 學權重用：設定時，求解結束後以每組權重重算最佳方案的 V3 總成本。
    static PROBES: std::cell::RefCell<Option<(Vec<&'static crate::scoring_v3::TuningV3>, Vec<f64>)>> =
        const { std::cell::RefCell::new(None) };
}

/// 學權重用：在 `f` 裡的求解結束時，以每組探測權重重算最佳方案的 V3 總成本
/// （同一條路線、不重新搜尋）。回傳最後一次求解的結果；沒有 V3 解時為空。
#[doc(hidden)]
pub fn with_probes<R>(
    probes: &[&'static crate::scoring_v3::TuningV3],
    f: impl FnOnce() -> R,
) -> (R, Vec<f64>) {
    let previous = PROBES.with(|cell| cell.replace(Some((probes.to_vec(), vec![]))));
    let result = f();
    let totals = PROBES
        .with(|cell| cell.replace(previous))
        .map(|(_, totals)| totals)
        .unwrap_or_default();
    (result, totals)
}

/// 先以「準時接觸優先」搜尋；若因此找不到方案，再允許所有 Touch 使用晚接觸重試一次。
/// 優先規則只是縮小候選的策略，可能剪掉必須晚接才走得通的路線。
pub fn solve(chart: &Chart, c: &SolverConfig) -> Result<Vec<Solution>, Diagnostic> {
    solve_traced(chart, c, None, None)
}

/// 真人標註的手部限制，以音符 id 指定。求解時每組同時音展開完，只保留新指派都符合的狀態，
/// 因此結果是「照這個手順打」時模型能找到的最佳動作與成本。
#[derive(Clone, Debug, Default)]
pub struct HandRules {
    /// Tap／Hold／Touch 的接觸，以及 Slide 起點觸碰。
    pub contact: std::collections::HashMap<String, Hand>,
    /// Slide 的追蹤手：依時間排序的 (起始秒數, 手)；第一筆從 −∞ 起算。空的表示兩手皆可（WiFi 2+1）。
    pub track: std::collections::HashMap<String, Vec<(f64, Hand)>>,
    /// 滑行標記沒有換手時間的 Slide → 最晚接上的時間。標註格式的 track 是「開始滑行的手」，
    /// 真人中途換手、放開後由另一手補完常不另外標；solve_constrained 先當成整條同一隻手，
    /// 走不下去時才把卡住的這類 Slide 放進 track_open。Temoto 的限制是整條同一隻手，不設。
    pub track_start: std::collections::HashMap<String, f64>,
    /// 只限定開始滑的手的 Slide：這個時刻之後開始的滑行兩手皆可。
    pub track_open: std::collections::HashMap<String, f64>,
    /// 同一時刻相鄰兩鍵的 Tap（含星星起點）都標成同一隻手時，允許那隻手張開一起按下。
    pub pair_press: bool,
    /// 不求解的音符部分（模型部分解略過的，見 solve_partial）：沒有任務、不指派手，
    /// 結果的 skipped 照列。
    pub skip: Vec<SkippedPart>,
}

/// 標註的換手時刻與模型 checkpoint 對不齊，前後這段時間內兩手都算符合。
pub const HANDOVER_TOLERANCE: f64 = 0.1;

/// 照標註求解走不下去時的說明。沒標換手時間的 Slide 走不下去時，solve_constrained 會自己
/// 放寬成只限定開始滑的手，所以不再請標記者補標換手時間。
pub const ANNOTATION_INFEASIBLE_MESSAGE: &str = "依標註的手順，模型在此時找不到能完成的動作（手被佔用、來不及移動，或 Slide 換手與模型的交接規則不合）。";

impl HandRules {
    /// 只略過這些部分、不限制手順：pair_press 打開，其餘與不帶限制的求解相同。
    pub fn skipping(skip: Vec<SkippedPart>) -> Self {
        Self {
            pair_press: true,
            skip,
            ..Self::default()
        }
    }

    fn allows(&self, a: &Assignment) -> bool {
        if a.part == "slide" {
            let Some(schedule) = self.track.get(&a.note_id) else {
                return true;
            };
            if schedule.is_empty()
                || schedule.iter().skip(1).any(|(at, _)| {
                    *at >= a.start_seconds - HANDOVER_TOLERANCE
                        && *at <= a.end_seconds + HANDOVER_TOLERANCE
                })
                || self
                    .track_open
                    .get(&a.note_id)
                    .is_some_and(|open| a.start_seconds > open + EPS)
            {
                return true;
            }
            let expected = schedule
                .iter()
                .take_while(|(at, _)| *at <= a.start_seconds)
                .last()
                .unwrap_or(&schedule[0])
                .1;
            return a.hand == expected;
        }
        self.contact
            .get(&a.note_id)
            .is_none_or(|hand| *hand == a.hand)
    }

    /// 這一組新增的指派裡不符合標註的音符（不重複）；擱著的標記（aside）不算。
    fn violations(&self, state: &State, aside: &BTreeSet<String>) -> Vec<String> {
        let mut out: Vec<String> = vec![];
        let mut cursor = state.assignments.head.as_ref();
        for _ in 0..state.assignments.len.saturating_sub(state.assign_mark) {
            let Some(link) = cursor else {
                break;
            };
            let a = &link.value;
            if !aside.contains(&a.note_id) && !self.allows(a) && !out.contains(&a.note_id) {
                out.push(a.note_id.clone());
            }
            cursor = link.prev.head.as_ref();
        }
        out
    }
}

/// 照標註略過模式（solve_constrained_skipping）：某一組只剩違反標註的展開時，擱著違反最少、
/// 目前最好的那一個所違反的標記，從這一組起照模型的手接下去。
#[derive(Default)]
struct Skipping {
    /// 擱著的標記（音符 id）。
    aside: BTreeSet<String>,
    /// 每一處擱置（code=annotation_skipped）。
    skipped: Vec<Diagnostic>,
}

/// 照標註略過時每一處的說明。
pub const ANNOTATION_SKIPPED_MESSAGE: &str = "依標註的手順，模型在此時找不到能完成的動作；這幾顆的標記先擱著、照模型的手接下去，後面的標註照常求解。";

/// 略過模式的退路：照標註走得下去的某一組裡，只違反標註的那些展開。
struct Fork {
    time: f64,
    /// 下一組的開頭。
    limit: usize,
    violators: Vec<(State, Vec<String>)>,
}

/// 擱著最好的那個展開所違反的標記並記下這一處，回傳只違反擱著標記的展開。
fn set_aside(
    skip: &mut Skipping,
    violators: Vec<(State, Vec<String>)>,
    time: f64,
    chart: &Chart,
    c: &SolverConfig,
) -> Vec<State> {
    let best = violators
        .iter()
        .min_by(|a, b| compare_states(&a.0, &b.0, c, false))
        .expect("只在有違反標註的展開時擱置");
    let mut ids = best.1.clone();
    ids.sort();
    skip.aside.extend(ids.iter().cloned());
    let mut d = Diagnostic::plain("annotation_skipped", ANNOTATION_SKIPPED_MESSAGE.into());
    d.severity = "warning".into();
    d.time_seconds = Some(time);
    d.source_span = chart
        .notes
        .iter()
        .find(|n| n.id == ids[0])
        .map(|n| Box::new(n.source_span.clone()));
    d.note_ids = ids;
    skip.skipped.push(d);
    violators
        .into_iter()
        .filter(|(_, v)| v.iter().all(|id| skip.aside.contains(id)))
        .map(|(state, _)| state)
        .collect()
}

/// 依真人標註限制手順求解。找不到符合的動作時回傳 code=annotation_infeasible 的診斷，
/// 標出第一個走不下去的時刻與音符。
///
/// 只標開始滑的手（track_start）的 Slide 先當成整條同一隻手；走不下去時，卡住那一組裡的這類
/// Slide 改成只限定開始滑的手、中途換手由模型決定，再試。卡住的地方沒有這類 Slide 就不放寬：
/// 每一輪都要整首重解，放寬不相干的 Slide 只會讓走不下去的譜面算更久。
pub fn solve_constrained(
    chart: &Chart,
    c: &SolverConfig,
    rules: &HandRules,
) -> Result<Vec<Solution>, Diagnostic> {
    constrained(chart, c, rules).0
}

/// solve_constrained 的本體；另外回傳最後用的限制（放寬過的滑行記在 track_open）。
fn constrained<'a>(
    chart: &Chart,
    c: &SolverConfig,
    rules: &'a HandRules,
) -> (
    Result<Vec<Solution>, Diagnostic>,
    std::borrow::Cow<'a, HandRules>,
) {
    const TARGETED_ROUNDS: usize = 3;
    let mut rules = std::borrow::Cow::Borrowed(rules);
    for round in 0.. {
        // 標註擋掉的往往是更早的接法（例如另一手接回）；那時還有「先等」的狀態撐著，
        // 真正走不下去的那一組沒有狀態被標註剔除，回報的就是 no_solution。
        let e = match solve_traced(chart, c, None, Some(&rules)) {
            Err(e) if matches!(e.code.as_str(), "annotation_infeasible" | "no_solution") => e,
            Ok(mut solutions) => {
                for solution in &mut solutions {
                    solution.warnings.extend(opened_warning(&rules));
                }
                return (Ok(solutions), rules);
            }
            other => return (other, rules),
        };
        let closed =
            |id: &String| rules.track_start.contains_key(id) && !rules.track_open.contains_key(id);
        let stuck: Vec<String> = if round < TARGETED_ROUNDS {
            e.note_ids.iter().filter(|id| closed(id)).cloned().collect()
        } else {
            vec![]
        };
        if stuck.is_empty() {
            return (Err(e), rules);
        }
        let rules = rules.to_mut();
        for id in stuck {
            let open = rules.track_start[&id];
            rules.track_open.insert(id, open);
        }
    }
    unreachable!()
}

fn opened_warning(rules: &HandRules) -> Option<String> {
    let opened = rules.track_open.len();
    (opened > 0).then(|| {
        format!(
            "有 {opened} 條 Slide 的滑行標記沒標換手時間，照整條同一隻手走不下去，改成只限定開始滑的手，中途換手由模型決定。"
        )
    })
}

/// 照標註求解，走不下去的地方不停下：那一刻違反的標記先擱著、照模型的手接下去，讓標記者
/// 看得到後面的標註。回傳結果與每一處擱置（code=annotation_skipped、severity=warning）；
/// 擱著以後仍走不下去（兩手都被佔用等模型本身的限制）時，結果是那一處的錯誤。
///
/// 照標註第一遍走得通就和 solve_constrained 相同，不擱置任何標記。走不通時整條放寬鏈都卡在
/// 同一處，所以直接用最後一輪的設定（Touch 可晚接、Slide 進最後判定區可離手、Hold 可提早
/// 放手）從頭解一遍，不重跑前面幾輪。
pub fn solve_constrained_skipping(
    chart: &Chart,
    c: &SolverConfig,
    rules: &HandRules,
) -> (Result<Vec<Solution>, Diagnostic>, Vec<Diagnostic>) {
    let (result, rules) = constrained(chart, c, rules);
    match &result {
        Err(e) if matches!(e.code.as_str(), "annotation_infeasible" | "no_solution") => {}
        _ => return (result, vec![]),
    }
    let last = SolverConfig {
        hold_early_release: true,
        ..c.clone()
    };
    let tracking;
    let chart = if c.is_v3() {
        tracking = tracking::tracking_chart(chart, c);
        &tracking
    } else {
        chart
    };
    let run = |config: &SolverConfig| {
        let mut skip = Skipping::default();
        let result = solve_with(
            chart,
            config,
            true,
            true,
            None,
            Some(&rules),
            Some(&mut skip),
            false,
        );
        (result, skip.skipped)
    };
    let (mut result, mut skipped) = run(&last);
    // 手掌半徑同 solve_traced：平常的大小走不下去時，才改用備用半徑。
    let mut wide = false;
    if matches!(&result, Err(e) if matches!(e.code.as_str(), "annotation_infeasible" | "no_solution"))
        && c.palm_radius > 0.0
        && c.palm_fallback_radius > c.palm_radius
    {
        let wider = SolverConfig {
            palm_radius: c.palm_fallback_radius,
            ..last.clone()
        };
        if let (Ok(solutions), list) = run(&wider) {
            (result, skipped, wide) = (Ok(solutions), list, true);
        }
    }
    let result = result.map(|mut solutions| {
        for solution in &mut solutions {
            if wide {
                solution.warnings.push(format!(
                    "手掌半徑 {} 找不到可行方案，此方案改用備用手掌半徑 {}。",
                    c.palm_radius, c.palm_fallback_radius
                ));
            }
            solution.warnings.extend(opened_warning(&rules));
            if !skipped.is_empty() {
                solution.warnings.push(format!(
                    "照標註有 {} 處走不下去，那幾顆的標記先擱著、照模型的手接下去；這個方案也允許 Touch 在判定區間內晚接、Slide 進最後判定區就離手、Hold 在結尾不檢查的區間內提早放手。",
                    skipped.len()
                ));
            }
        }
        solutions
    });
    (result, skipped)
}

/// 部分解每一處的說明（比對結果的 model_skipped）。
pub const MODEL_SKIPPED_MESSAGE: &str =
    "模型在此時走不下去（手被佔用、來不及移動等），這幾顆先略過、不指派手，其餘照常求解。";

/// 模型本身走不下去時的部分解：略過做不到的音符部分，其餘照常求解；略過的部分列在每個方案的
/// skipped。整首解得出來時與 solve 相同；只有計算預算、取消之類的錯誤才回傳錯誤。
///
/// 第一遍直接用放寬鏈最後一輪的設定（Touch 可晚接、Slide 進最後判定區可離手、Hold 可提早放手）
/// 從頭解：走不下去的那一組從原本的 beam 重來，允許略過音符，先只略過新的接觸，再含還沒接上
/// 的 Slide，最後才放掉進行中的 Slide，略過數由少到多，留略過最少的狀態接下去。第二遍把找到
/// 的部分當成不存在，照平常的放寬鏈重解，其餘部分和完整譜面一樣從最嚴格的設定開始；第二遍
/// 仍走不下去時，改用第一遍的結果（拿掉略過部分已有的指派）。
pub fn solve_partial(chart: &Chart, c: &SolverConfig) -> Result<Vec<Solution>, Diagnostic> {
    let error = match solve(chart, c) {
        Err(e) if e.code == "no_solution" => e,
        other => return other,
    };
    let count = |found: &[Solution]| found.first().map_or(usize::MAX, |s| s.skipped.len());
    let mut found = discover(chart, c, c.palm_radius)?;
    // 手掌半徑同 solve_traced：平常的大小要略過時也試備用半徑，取略過較少的。
    if count(&found) > 0 && c.palm_radius > 0.0 && c.palm_fallback_radius > c.palm_radius {
        if let Ok(wide) = discover(chart, c, c.palm_fallback_radius) {
            if count(&wide) < count(&found) {
                found = wide;
            }
        }
    }
    let Some(skipped) = found.first().map(|s| s.skipped.clone()) else {
        return Err(error);
    };
    if skipped.is_empty() {
        return Ok(found);
    }
    let mut solutions = match solve_traced(chart, c, None, Some(&HandRules::skipping(skipped.clone()))) {
        Ok(solutions) => solutions,
        Err(e) if e.code == "no_solution" => found
            .into_iter()
            .map(|mut solution| {
                without_skipped(&mut solution);
                solution.warnings.push(
                    "這個方案允許 Touch 在判定區間內晚接、Slide 進最後判定區就離手、Hold 在結尾不檢查的區間內提早放手。".into(),
                );
                solution
            })
            .collect(),
        Err(e) => return Err(e),
    };
    let warning = partial_warning(&skipped);
    for solution in &mut solutions {
        solution.warnings.extend(warning.clone());
    }
    Ok(solutions)
}

/// 部分解的第一遍：放寬鏈最後一輪的設定，整組走不下去時略過音符接下去。
fn discover(
    chart: &Chart,
    c: &SolverConfig,
    palm_radius: f64,
) -> Result<Vec<Solution>, Diagnostic> {
    let last = SolverConfig {
        hold_early_release: true,
        palm_radius,
        ..c.clone()
    };
    let tracking;
    let chart = if c.is_v3() {
        tracking = tracking::tracking_chart(chart, c);
        &tracking
    } else {
        chart
    };
    solve_with(chart, &last, true, true, None, None, None, true)
}

/// 拿掉略過部分已有的指派與交接（第一遍中途放掉的 Slide 前半段）。
fn without_skipped(solution: &mut Solution) {
    let skipped = std::mem::take(&mut solution.skipped);
    let gone = |note: &str, slide: bool| {
        skipped
            .iter()
            .any(|p| p.note_id == note && (p.part == "slide") == slide)
    };
    solution
        .assignments
        .retain(|a| !gone(&a.note_id, a.part == "slide"));
    solution.handovers.retain(|h| !gone(&h.note_id, true));
    solution.skipped = skipped;
}

/// 依走不下去的時刻分成幾處：每處的時刻與音符 id（不重複，依時間、再依 id）。
fn skipped_spots(skipped: &[SkippedPart]) -> Vec<(f64, Vec<String>)> {
    let mut parts: Vec<&SkippedPart> = skipped.iter().collect();
    parts.sort_by(|a, b| a.time_seconds.total_cmp(&b.time_seconds));
    let mut out: Vec<(f64, Vec<String>)> = vec![];
    for part in parts {
        match out.last_mut() {
            Some((time, ids)) if (part.time_seconds - *time).abs() < 1e-6 => {
                if !ids.contains(&part.note_id) {
                    ids.push(part.note_id.clone());
                }
            }
            _ => out.push((part.time_seconds, vec![part.note_id.clone()])),
        }
    }
    out
}

/// 部分解的提醒：有幾處走不下去、先略過。完整的方案沒有。
pub fn partial_warning(skipped: &[SkippedPart]) -> Option<String> {
    let spots = skipped_spots(skipped).len();
    (spots > 0).then(|| {
        format!(
            "模型有 {spots} 處走不下去（手被佔用、來不及移動等），那幾顆先略過、不指派手，其餘照常求解。"
        )
    })
}

/// 部分解略過的每一處（code=model_skipped、severity=warning）：時刻、音符與第一顆的原文位置。
pub fn skipped_diagnostics(chart: &Chart, skipped: &[SkippedPart]) -> Vec<Diagnostic> {
    skipped_spots(skipped)
        .into_iter()
        .map(|(time, ids)| {
            let mut d = Diagnostic::plain("model_skipped", MODEL_SKIPPED_MESSAGE.into());
            d.severity = "warning".into();
            d.time_seconds = Some(time);
            d.source_span = chart
                .notes
                .iter()
                .find(|n| n.id == ids[0])
                .map(|n| Box::new(n.source_span.clone()));
            d.note_ids = ids;
            d
        })
        .collect()
}

/// V3 debug: replay ownership, roles and lookahead parts of the Top-1 candidate.
pub fn trace_v3(chart: &Chart, c: &SolverConfig) -> Result<Vec<String>, Diagnostic> {
    let mut lines = vec![];
    solve_traced(chart, c, Some(&mut lines), None)?;
    Ok(lines)
}

fn solve_traced(
    chart: &Chart,
    c: &SolverConfig,
    mut trace: Option<&mut Vec<String>>,
    rules: Option<&HandRules>,
) -> Result<Vec<Solution>, Diagnostic> {
    let result = solve_relaxing_holds(chart, c, trace.as_deref_mut(), rules);
    // 手掌半徑以平常的大小為主：整首都走不通時，才改用較大的備用半徑重試並註明。
    match result {
        Err(e)
            if c.palm_radius > 0.0
                && c.palm_fallback_radius > c.palm_radius
                && matches!(e.code.as_str(), "no_solution" | "annotation_infeasible") =>
        {
            let wider = SolverConfig {
                palm_radius: c.palm_fallback_radius,
                ..c.clone()
            };
            // 備用半徑也走不通時，回報走得比較遠的那一處（開頭的問題已被備用半徑解掉時，
            // 使用者需要看到的是後面真正卡住的地方）。
            let mut solutions =
                solve_relaxing_holds(chart, &wider, trace, rules).map_err(|wide| {
                    // 備用半徑算到上限（search_limit）不算走得比較遠，仍回報原本走不下去的地方。
                    let later =
                        matches!(wide.code.as_str(), "no_solution" | "annotation_infeasible")
                            && wide.time_seconds.unwrap_or(f64::NEG_INFINITY)
                                > e.time_seconds.unwrap_or(f64::INFINITY);
                    if later {
                        let mut wide = wide;
                        wide.message = format!(
                            "（已改用備用手掌半徑 {} 重試）{}",
                            c.palm_fallback_radius, wide.message
                        );
                        wide
                    } else {
                        e
                    }
                })?;
            for solution in &mut solutions {
                solution.warnings.push(format!(
                    "手掌半徑 {} 找不到可行方案，此方案改用備用手掌半徑 {}。",
                    c.palm_radius, c.palm_fallback_radius
                ));
            }
            Ok(solutions)
        }
        other => other,
    }
}

fn solve_relaxing_holds(
    chart: &Chart,
    c: &SolverConfig,
    mut trace: Option<&mut Vec<String>>,
    rules: Option<&HandRules>,
) -> Result<Vec<Solution>, Diagnostic> {
    let result = solve_chain(chart, c, trace.as_deref_mut(), rules);
    // Hold 一律按完是偏好不是硬規則：整首都找不到方案時才允許提早放手，並註明。
    match result {
        Err(e)
            if !c.hold_early_release
                && matches!(e.code.as_str(), "no_solution" | "annotation_infeasible") =>
        {
            let relaxed = SolverConfig {
                hold_early_release: true,
                ..c.clone()
            };
            let mut solutions = solve_chain(chart, &relaxed, trace, rules).map_err(|_| e)?;
            for solution in &mut solutions {
                solution.warnings.push(
                    "Hold 一律按到結尾時找不到可行方案，此方案允許 Hold 在結尾不檢查的區間內提早放手。".into(),
                );
            }
            Ok(solutions)
        }
        other => other,
    }
}

fn solve_chain(
    chart: &Chart,
    c: &SolverConfig,
    mut trace: Option<&mut Vec<String>>,
    rules: Option<&HandRules>,
) -> Result<Vec<Solution>, Diagnostic> {
    // V3 依判定區抄近：手部路線與完成時間換成追蹤用譜面，回傳的譜面不變。
    let tracking;
    let chart = if c.is_v3() {
        tracking = tracking::tracking_chart(chart, c);
        &tracking
    } else {
        chart
    };
    let retry = |e: &Diagnostic| e.code == "no_solution" || e.code == "annotation_infeasible";
    match solve_with(
        chart,
        c,
        false,
        false,
        trace.as_deref_mut(),
        rules,
        None,
        false,
    ) {
        Err(e) if retry(&e) => {}
        other => return other,
    }
    // V3 預設畫完：放寬時先只允許 Touch 晚接，仍然無解才允許 Slide 進最後判定區就離手。
    let draw = c.is_v3() && !c.slide_shortcut;
    let mut solutions = match solve_with(
        chart,
        c,
        true,
        !draw,
        trace.as_deref_mut(),
        rules,
        None,
        false,
    ) {
        Err(e) if draw && retry(&e) => solve_with(chart, c, true, true, trace, rules, None, false),
        other => other,
    }?;
    for solution in &mut solutions {
        solution
            .warnings
            .push("準時接觸優先的搜尋無解，此方案允許 Touch 在判定區間內任意晚接觸。".into());
    }
    Ok(solutions)
}

// 放寬鏈的兩個開關、兩種走不下去時的處理（擱著標記、略過音符）各自獨立，分開傳比較清楚。
#[allow(clippy::too_many_arguments)]
fn solve_with(
    chart: &Chart,
    c: &SolverConfig,
    relaxed: bool,
    // 允許 Slide 進最後判定區後、為了別的音符就離手（不管是否必要）。
    leave: bool,
    trace: Option<&mut Vec<String>>,
    rules: Option<&HandRules>,
    // 照標註略過模式：只違反標註的那一組不停下，擱著標記接下去。
    mut skip: Option<&mut Skipping>,
    // 部分解的第一遍（solve_partial）：整組走不下去時略過音符接下去。
    drop: bool,
) -> Result<Vec<Solution>, Diagnostic> {
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
    let v3_context = if c.is_v3() {
        Some(v3::Context::new(chart, c)?)
    } else {
        None
    };
    let mut beam = vec![State {
        v2: v2::Data::default(),
        v3: v3::Data::default(),
        actions: Chain::default(),
        group_origin: 0,
        arms,
        cost: CostBreakdown::default(),
        assignments: Chain::default(),
        handovers: Chain::default(),
        palms: Chain::default(),
        active_palms: [None, None],
        held_touch: [None, None],
        owners: BTreeMap::new(),
        wifi_pairs: BTreeMap::new(),
        engaged: BTreeMap::new(),
        pending: 0.0,
        deposit: BTreeMap::new(),
        last_handover: BTreeMap::new(),
        used_touch_sweep: false,
        lenient_in_group: false,
        finished: std::collections::BTreeSet::new(),
        suspended: BTreeMap::new(),
        resumed: std::collections::BTreeSet::new(),
        bundled: BTreeMap::new(),
        bundle_lead: BTreeMap::new(),
        used_bundle: false,
        carried: BTreeMap::new(),
        committed: BTreeMap::new(),
        used_carry: false,
        swept: BTreeMap::new(),
        used_sweep: false,
        used_pair_press: false,
        assign_mark: 0,
        forced_handovers: Arc::new(
            rules
                .map(|r| {
                    r.track
                        .iter()
                        .filter(|(_, schedule)| schedule.len() > 1)
                        .map(|(id, _)| id.clone())
                        .collect()
                })
                .unwrap_or_default(),
        ),
        single_track: Arc::new(
            rules
                .map(|r| {
                    r.track
                        .iter()
                        .filter(|(_, schedule)| !schedule.is_empty())
                        .map(|(id, _)| id.clone())
                        .collect()
                })
                .unwrap_or_default(),
        ),
        used_early_slide: false,
        used_touch_group: false,
        dropped: Chain::default(),
    }];
    if let Some(context) = &v2_context {
        context.refresh(&mut beam[0])?;
    }
    if let Some(context) = &v3_context {
        context.refresh(&mut beam[0])?;
    }
    // 部分解已略過的部分（rules.skip）：不建任務、不算新接觸，結果照列在 skipped。
    let given: &[SkippedPart] = rules.map_or(&[], |r| r.skip.as_slice());
    let mut left_out = vec![[false; 2]; chart.notes.len()];
    for part in given {
        if let Some(i) = chart.notes.iter().position(|n| n.id == part.note_id) {
            left_out[i][usize::from(part.part == "slide")] = true;
        }
    }
    let tasks: Vec<Task> = tasks(chart, c)?
        .into_iter()
        .filter(|t| !left_out[t.note][usize::from(t.mode == "slide")])
        .collect();
    // 需要新接觸的時刻（Tap、Hold、Touch 與 Slide 起點），用來預判手是否需要提早離開 Slide。
    let mut onsets: Vec<f64> = chart
        .notes
        .iter()
        .enumerate()
        .filter(|(i, n)| n.has_head && !left_out[*i][0])
        .map(|(_, n)| n.time_seconds)
        .collect();
    onsets.sort_by(f64::total_cmp);
    let clock = Instant::now();
    let mut expansions: u64 = 0;
    let no_aside = BTreeSet::new();
    // 略過模式的退路：最近幾組照標註走得下去時，同一組只違反標註的展開（本來就要丟掉，直接
    // 留著不必複製）。之後走不下去、卻沒有展開被標註擋掉時（例如暫停的 Slide 到最後才接不
    // 回來），退回最近的一組，擱著那裡的標記、改走這些展開。
    let mut forks: Vec<Fork> = vec![];
    let mut backtracks = 0usize;
    // 正在退回的那一處走不下去的時刻，與退回前已有的擱置數。
    let mut stuck: Option<(f64, usize)> = None;
    let mut cursor = 0;
    'groups: while cursor < tasks.len() {
        let time = tasks[cursor].start;
        let mut limit = cursor + 1;
        while limit < tasks.len() && (tasks[limit].start - time).abs() < EPS {
            limit += 1;
        }
        let group = &tasks[cursor..limit];
        if stuck.is_some_and(|(at, _)| time > at + EPS) {
            stuck = None;
        }
        // 已經結束的 Slide 不會再被查詢；清掉之後每個狀態要複製的資料量才是常數。
        for (origin, state) in beam.iter_mut().enumerate() {
            state.group_origin = origin;
            state.assign_mark = state.assignments.len;
            state.lenient_in_group = false;
            let live =
                |note: &usize| chart.notes[*note].motion_end.unwrap_or(f64::INFINITY) >= time - EPS;
            state.owners.retain(|note, _| live(note));
            state.wifi_pairs.retain(|note, _| live(note));
            state.engaged.retain(|note, _| live(note));
            state.finished.retain(&live);
            state.suspended.retain(|note, _| live(note));
            state.resumed.retain(&live);
            state.bundled.retain(|note, _| live(note));
            state.bundle_lead.retain(|note, _| live(note));
            state.carried.retain(|note, _| live(note));
            state.swept.retain(|note, _| live(note));
            state
                .committed
                .retain(|note, until| live(note) && *until >= time - EPS);
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
            let aside = skip.as_deref().map_or(&no_aside, |s| &s.aside);
            let mut swept = Vec::new();
            for state in &beam {
                for reversed in [false, true] {
                    if let Some(mut result) = assign_touch_sweep(state, group, chart, c, reversed) {
                        if let Some(context) = &v2_context {
                            context.update(state, &mut result)?;
                        }
                        if let Some(context) = &v3_context {
                            context.update(state, &mut result)?;
                        }
                        if rules.is_none_or(|r| r.violations(&result, aside).is_empty()) {
                            swept.push(result);
                        }
                    }
                }
            }
            if !swept.is_empty() {
                beam = swept;
                if let Some(context) = &v3_context {
                    context.plan(&mut beam, time)?;
                }
                prune(&mut beam, c, time);
                cursor = limit;
                continue;
            }
        }
        // 這組的順序被浮點誤差打亂時，先照原順序展開（結果與校準時相同）；整組都走不通才改用
        // 標準順序重來一次。順序只是搜尋策略，不同順序能展開出的接法不完全一樣。
        let mut retry = canonical_group(group, chart).map(|order| (order, beam.clone()));
        let mut reordered: Option<Vec<Task>> = None;
        // 一手同按相鄰兩鍵：標記者標成同一隻手的照標註展開；其餘只在整首最後的放寬重試裡、
        // 這一組兩手怎麼分都走不下去時，才帶著原本的 beam 重來一次，每一對都展開。
        let last_pass = relaxed && leave && c.hold_early_release;
        let mut paired = (last_pass
            && rules.is_none_or(|r| r.pair_press)
            && !adjacent_pairs(group, chart).is_empty())
        .then(|| beam.clone());
        let mut every_pair = false;
        // 部分解：整組怎麼排都走不下去時，從這一組開始時的 beam 重來，允許略過音符。
        // (每個狀態在這一組最多略過幾個, 可以略過到哪一層)：0 只略過新的接觸，
        // 1 再含還沒接上的 Slide，2 才放掉進行中的 Slide；略過數由少到多。
        let dropping = drop.then(|| beam.clone());
        let mut drop_pass: Option<(usize, u8)> = None;
        // 略過模式：只違反標註的完整展開（違反最少的那些）先留著，連同違反的音符。
        let mut violators: Vec<(State, Vec<String>)> = vec![];
        let mut next = loop {
            let group = reordered.as_deref().unwrap_or(group);
            let aside = skip.as_deref().map_or(&no_aside, |s| &s.aside);
            let candidates = palm_candidates(group, chart, c.palm_radius);
            let labeled = |pair: &[usize; 2]| {
                rules.is_some_and(|r| {
                    let [a, b] = pair.map(|i| r.contact.get(&chart.notes[group[i].note].id));
                    r.pair_press && a.is_some() && a == b
                })
            };
            let pairs: Vec<[usize; 2]> = adjacent_pairs(group, chart)
                .into_iter()
                .filter(|pair| every_pair || labeled(pair))
                .collect();
            let touch_groups = touch_groups(group, chart);
            let in_touch_group: Vec<bool> = (0..group.len())
                .map(|i| touch_groups.iter().any(|g| g.contains(&i)))
                .collect();
            let mut next = vec![];
            // 依標註剔除的狀態數；整組都被剔除時要回報是標註走不通，而不是模型本身無解。
            let mut rejected = 0usize;
            // 整組都是 Touch 又有 Touch Group：逐顆展開走不下去時改列舉兩手的動作（touch_slap），
            // 要用這一組開始時的 beam。
            let slap = (drop_pass.is_none()
                && !touch_groups.is_empty()
                && group
                    .iter()
                    .all(|t| t.mode == "tap" && chart.notes[t.note].kind == "touch"))
            .then(|| beam.clone());
            // 顆數多的整組 Touch 直接列舉兩手的動作，找得到就不逐顆展開（逐顆展開的分支隨顆數
            // 指數成長）：先只用每團正中間的掌心，不行再試全部掌心，都不行才逐顆展開。
            if group.len() >= SLAP_FIRST_NOTES {
                if let Some(saved) = &slap {
                    for centered in [true, false] {
                        next = touch_slap(
                            saved,
                            group,
                            chart,
                            c,
                            relaxed,
                            centered,
                            &candidates,
                            &touch_groups,
                            rules,
                            aside,
                            v2_context.as_ref(),
                            v3_context.as_ref(),
                        )?;
                        if !next.is_empty() {
                            break;
                        }
                    }
                }
            }
            // 第三個欄位標記由 Touch Group 過半判定連帶完成、沒有實際接觸的 Touch。
            let mut pending: Vec<(State, Vec<bool>, Vec<bool>)> = if next.is_empty() {
                std::mem::take(&mut beam)
                    .into_iter()
                    .map(|state| (state, vec![true; group.len()], vec![false; group.len()]))
                    .collect()
            } else {
                vec![]
            };
            while let Some((state, remaining, auto)) = pending.pop() {
                if cancelled() {
                    return Err(Diagnostic::plain("cancelled", "分析已取消".into()));
                }
                if expansions > MAX_EXPANSIONS
                    || clock.elapsed() > BUDGET
                    || pending.len() + next.len() > MAX_GROUP_STATES
                {
                    return Err(group_diagnostic(
                        "search_limit",
                        SEARCH_LIMIT_MESSAGE.into(),
                        time,
                        group,
                        chart,
                    ));
                }
                // Touch Group 成員最後展開：其他音符先分到手，剩下接不到的 Touch 才交給過半判定。
                let next_task = (0..group.len())
                    .find(|j| remaining[*j] && !in_touch_group[*j])
                    .or_else(|| remaining.iter().position(|todo| *todo));
                let Some(i) = next_task else {
                    let Some(mut state) =
                        settle_touch_groups(state, &auto, &touch_groups, group, chart, rules)
                    else {
                        continue;
                    };
                    if let Some(context) = &v2_context {
                        context.refresh(&mut state)?;
                    }
                    if let Some(context) = &v3_context {
                        context.refresh(&mut state)?;
                    }
                    let violated = rules.map_or(vec![], |r| r.violations(&state, aside));
                    if !violated.is_empty() {
                        rejected += 1;
                        if skip.is_some() {
                            let fewest = violators.first().map_or(usize::MAX, |(_, v)| v.len());
                            if violated.len() < fewest {
                                violators.clear();
                            }
                            if violated.len() <= fewest {
                                violators.push((state, violated));
                            }
                        }
                        continue;
                    }
                    next.push(state);
                    continue;
                };
                let task = &group[i];
                let mut after = remaining.clone();
                after[i] = false;
                if task.mode == "slide"
                    && (state.finished.contains(&task.note)
                        || state.bundled.contains_key(&task.note)
                        || state.swept.contains_key(&task.note)
                        || riding(&state, task, chart))
                {
                    pending.push((state, after, auto));
                    continue;
                }
                if let Some((limit, tier)) = drop_pass {
                    let level = match task.mode {
                        "slide"
                            if state.owners.contains_key(&task.note)
                                || state.engaged.contains_key(&task.note)
                                || state.suspended.contains_key(&task.note) =>
                        {
                            2
                        }
                        "slide" => 1,
                        _ => 0,
                    };
                    if level <= tier && drops_at(&state, time) < limit {
                        pending.push((drop_part(&state, task, time), after.clone(), auto.clone()));
                    }
                }
                if let Some(&(progress, bound)) = state.suspended.get(&task.note) {
                    for hand in [Hand::L, Hand::R] {
                        expansions += 1;
                        let Some(resumed) = resume_slide(&state, task, hand, progress, chart)
                        else {
                            continue;
                        };
                        for mut s in assign(&resumed, task, hand, chart, c, false) {
                            if let Some(context) = &v3_context {
                                context.update(&state, &mut s)?;
                            }
                            pending.push((s, after.clone(), auto.clone()));
                        }
                    }
                    // 最後一段一定要有手接回去；其餘時間可以繼續等：這段路徑的移動量，
                    // 以及「晚一段才接回」多出來的壓縮成本，先記進排序用的預存。
                    if !task.last {
                        if let Some(context) = &v3_context {
                            let next_bound =
                                context.resume_bound(task, chart, progress, task.end)?;
                            let share = context.deposit(task, chart)? + next_bound - bound;
                            let mut waiting = state.clone();
                            waiting.pending += share;
                            *waiting.deposit.entry(task.note).or_default() += share;
                            waiting.suspended.insert(task.note, (progress, next_bound));
                            pending.push((waiting, after, auto));
                        }
                    }
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
                            if let Some(context) = &v3_context {
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
                            if let Some(context) = &v3_context {
                                context.update(&state, &mut s)?;
                            }
                            pending.push((s, after.clone(), auto.clone()));
                        }
                        expansions += 1;
                        if let Some(mut s) = brush_touch(&state, task, hand, chart, c) {
                            if let Some(context) = &v2_context {
                                context.update(&state, &mut s)?;
                            }
                            if let Some(context) = &v3_context {
                                context.update(&state, &mut s)?;
                            }
                            pending.push((s, after.clone(), auto.clone()));
                        }
                    }
                    for pair in pairs
                        .iter()
                        .filter(|p| !late && p.contains(&i) && p.iter().all(|j| remaining[*j]))
                    {
                        let mut after_pair = remaining.clone();
                        for j in pair {
                            after_pair[*j] = false;
                        }
                        for hand in [Hand::L, Hand::R] {
                            expansions += 1;
                            for mut s in pair_press(&state, *pair, group, hand, chart, c) {
                                if let Some(context) = &v2_context {
                                    context.update(&state, &mut s)?;
                                }
                                if let Some(context) = &v3_context {
                                    context.update(&state, &mut s)?;
                                }
                                pending.push((s, after_pair.clone(), auto.clone()));
                            }
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
                            let variants: Vec<PalmCandidate> = if !c.is_legacy() {
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
                                for mut s in
                                    assign_palm(&state, variant, group, hand, chart, c, late)
                                {
                                    s.lenient_in_group |= late;
                                    if let Some(context) = &v2_context {
                                        context.update(&state, &mut s)?;
                                    }
                                    if let Some(context) = &v3_context {
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
                        <= chart.notes[task.note].motion_start.unwrap()
                            + c.slide_pickup_seconds
                            + EPS;
                // Late pickup resolves an occupied hand or another simultaneous
                // contact. It is not a way to shorten an uncontested Slide's
                // opposite-side exposure for free below the comfort threshold.
                let pickup_conflict = c.is_legacy()
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
                    } else if let Some(context) = &v3_context {
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
                // V3 預設畫完：不為了少畫一段而提早離手（交接、暫停接回照常），
                // 只有整譜重試時才放寬。
                let may_leave = leave || needed_elsewhere && (!c.is_v3() || c.slide_shortcut);
                if task.mode == "slide"
                    && may_leave
                    && slide_judged(&state, task, chart)
                    && !locked(&state, task.note, task.start)
                {
                    let mut done = state.clone();
                    done.finished.insert(task.note);
                    done.owners.remove(&task.note);
                    done.engaged.remove(&task.note);
                    done.used_early_slide = true;
                    pending.push((done, after.clone(), auto.clone()));
                }
                // V3：還沒進最後判定區、這隻手在 Slide 結束前另有用途時，可以先放開，
                // 判定進度留在這裡，之後再接回去（真人常見的「滑一半、去打別的、再補完」）。
                if c.is_v3()
                    && task.mode == "slide"
                    && needed_elsewhere
                    && !task.last
                    && task.samples.is_empty()
                    && !state.resumed.contains(&task.note)
                    && !state.wifi_pairs.contains_key(&task.note)
                    && !state.bundle_lead.contains_key(&task.note)
                    && state.owners.contains_key(&task.note)
                    && !locked(&state, task.note, task.start)
                    && !slide_judged(&state, task, chart)
                {
                    if let (Some(&pickup), Some(end)) = (
                        state.engaged.get(&task.note),
                        chart.notes[task.note].motion_end,
                    ) {
                        let progress = (task.start - pickup) / (end - pickup);
                        if progress > EPS
                            && progress < 1.0 - EPS
                            && end - task.start >= SUSPEND_MIN_REMAINING - EPS
                        {
                            if let Some(context) = &v3_context {
                                let bound =
                                    context.resume_bound(task, chart, progress, task.end)?;
                                let share = context.deposit(task, chart)? + bound;
                                let mut paused = state.clone();
                                paused.owners.remove(&task.note);
                                paused.engaged.remove(&task.note);
                                paused.suspended.insert(task.note, (progress, bound));
                                paused.pending += share;
                                *paused.deposit.entry(task.note).or_default() += share;
                                pending.push((paused, after.clone(), auto.clone()));
                            }
                        }
                    }
                }
                // V3：判定佇列相同、較晚出發的另一條 Slide 還沒出發時，這隻手快要去打別的音符就可以
                // 先放開這條、不再接回：之後畫那一條的手會依序碰到這條剩下的判定區，並在這條的正解
                // 區間內完成（真人「先畫一段、去打別的、再一筆畫完兩條」）。不受暫停的剩餘時間限制。
                if c.is_v3()
                    && task.mode == "slide"
                    && needed_elsewhere
                    && (onsets
                        .get(onsets.partition_point(|t| *t <= task.start + EPS))
                        .is_some_and(|t| *t <= task.start + SWEEP_LEAD + EPS)
                        || after
                            .iter()
                            .enumerate()
                            .any(|(j, todo)| *todo && group[j].mode != "slide"))
                    && task.samples.is_empty()
                    && !state.wifi_pairs.contains_key(&task.note)
                    && !state.bundle_lead.contains_key(&task.note)
                    && state.owners.contains_key(&task.note)
                    && !locked(&state, task.note, task.start)
                    && !slide_judged(&state, task, chart)
                {
                    let lead = chart.paths[task.path]
                        .hand_routes
                        .sweepers
                        .iter()
                        .copied()
                        .find(|l| chart.notes[*l].motion_start.unwrap() >= task.start - EPS);
                    if let Some(lead) = lead {
                        let mut left = state.clone();
                        left.owners.remove(&task.note);
                        left.engaged.remove(&task.note);
                        left.swept.insert(task.note, lead);
                        left.used_sweep = true;
                        pending.push((left, after.clone(), auto.clone()));
                    }
                }
                // 沒有任何一手接得到的 Touch，交給 Touch Group 過半判定；整組結束時再驗證過半。
                if pending.len() == before && in_touch_group[i] {
                    let mut grouped = auto.clone();
                    grouped[i] = true;
                    pending.push((state, after, grouped));
                }
                if cancelled() {
                    return Err(Diagnostic::plain("cancelled", "分析已取消".into()));
                }
                if expansions > MAX_EXPANSIONS
                    || clock.elapsed() > BUDGET
                    || pending.len() + next.len() > MAX_GROUP_STATES
                {
                    return Err(group_diagnostic(
                        "search_limit",
                        SEARCH_LIMIT_MESSAGE.into(),
                        time,
                        group,
                        chart,
                    ));
                }
            }
            // 逐顆展開（含換順序重來）都走不下去的整組 Touch：改列舉兩手的動作（大組上面試過了）。
            if next.is_empty()
                && retry.is_none()
                && paired.is_none()
                && group.len() < SLAP_FIRST_NOTES
            {
                if let Some(saved) = &slap {
                    next = touch_slap(
                        saved,
                        group,
                        chart,
                        c,
                        relaxed,
                        false,
                        &candidates,
                        &touch_groups,
                        rules,
                        aside,
                        v2_context.as_ref(),
                        v3_context.as_ref(),
                    )?;
                }
            }
            if next.is_empty() {
                if let Some((order, saved)) = retry.take() {
                    reordered = Some(order);
                    beam = saved;
                    continue;
                }
                if let Some(saved) = paired.take() {
                    every_pair = true;
                    beam = saved;
                    continue;
                }
                // 略過模式：這一組只有違反標註的展開時，擱著標記接著解。
                if let (Some(skip), false) = (skip.as_deref_mut(), violators.is_empty()) {
                    break set_aside(skip, violators, time, chart, c);
                }
                // 沒有展開被標註擋掉：退回最近的岔路，擱著那裡的標記、改走當時違反標註的展開。
                if let (Some(skip), true) = (skip.as_deref_mut(), backtracks < MAX_BACKTRACKS) {
                    if let Some(fork) = forks.pop() {
                        stuck.get_or_insert((time, skip.skipped.len()));
                        backtracks += 1;
                        let mut next = set_aside(skip, fork.violators, fork.time, chart, c);
                        if c.is_v2() {
                            v2::remove_unnecessary_deferrals(&mut next);
                        } else if c.is_v3() {
                            v3::remove_unnecessary_deferrals(&mut next);
                        }
                        if let Some(context) = &v3_context {
                            context.plan(&mut next, fork.time)?;
                        }
                        prune(&mut next, c, fork.time);
                        beam = next;
                        cursor = fork.limit;
                        continue 'groups;
                    }
                }
                // 部分解：改從這一組開始時的 beam 重來，允許略過音符（見 drop_pass）。
                if let Some(saved) = &dropping {
                    let deepest = if group.iter().any(|t| t.mode == "slide") {
                        2
                    } else {
                        0
                    };
                    let pass = match drop_pass {
                        None => Some((1, 0)),
                        Some((limit, tier)) if tier < deepest => Some((limit, tier + 1)),
                        Some((limit, _)) if limit < group.len() => Some((limit + 1, 0)),
                        Some(_) => None,
                    };
                    if let Some(pass) = pass {
                        drop_pass = Some(pass);
                        beam = saved.clone();
                        continue;
                    }
                }
                // 退到底仍走不下去：退回途中擱著的標記不是原因，不列出。
                if let (Some(skip), Some((_, kept))) = (skip.as_deref_mut(), stuck) {
                    skip.skipped.truncate(kept);
                }
                let (code, message) = if rejected > 0 {
                    (
                        "annotation_infeasible",
                        ANNOTATION_INFEASIBLE_MESSAGE.into(),
                    )
                } else {
                    (
                        "no_solution",
                        "本模型未找到可行方案：兩手在此時都被佔用，或無法在判定區間內連續接觸。這不代表人類無法遊玩；可嘗試增加 beamWidth、palmRadius 或 glideDistance，或縮短片段。".into(),
                    )
                };
                return Err(group_diagnostic(code, message, time, group, chart));
            }
            if drop_pass.is_some() {
                // 略過最少的才接下去（帶著別條一起畫的 Slide 放掉時會連帶多略過）。
                let fewest = next.iter().map(|s| drops_at(s, time)).min().unwrap_or(0);
                next.retain(|s| drops_at(s, time) == fewest);
            }
            if skip.is_some() && !violators.is_empty() {
                let keep = 2 * c.beam_width;
                if violators.len() > keep {
                    violators
                        .select_nth_unstable_by(keep, |a, b| compare_states(&a.0, &b.0, c, false));
                    violators.truncate(keep);
                }
                forks.retain(|fork| fork.time >= time - FORK_WINDOW);
                if forks.len() >= MAX_FORKS {
                    forks.remove(0);
                }
                forks.push(Fork {
                    time,
                    limit,
                    violators,
                });
            }
            break next;
        };
        if c.is_v2() {
            v2::remove_unnecessary_deferrals(&mut next);
        } else if c.is_v3() {
            v3::remove_unnecessary_deferrals(&mut next);
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
        if let Some(context) = &v3_context {
            context.plan(&mut next, time)?;
        }
        prune(&mut next, c, time);
        beam = next;
        cursor = limit;
    }
    let end = chart
        .duration_seconds
        .max(tasks.iter().map(|t| t.end).fold(0.0, f64::max));
    for state in &mut beam {
        // V3 counts idle posture (sustained crossing), so the closing idle
        // pieces go through the same incremental update as everything else.
        let before = v3_context.as_ref().map(|_| state.clone());
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
        } else if let Some(context) = &v3_context {
            context.update(before.as_ref().unwrap(), state)?;
            context.verify(state)?;
        } else {
            let left = state.arms[0].segments.to_vec();
            let right = state.arms[1].segments.to_vec();
            state.cost.cross = cross_cost(&left, &right, c);
        }
    }
    beam.sort_by(|a, b| compare_states(a, b, c, true));
    if let (Some(lines), Some(context), Some(best)) = (trace, &v3_context, beam.first()) {
        *lines = context.trace(best)?;
    }
    if let (true, Some(best)) = (c.is_v3(), beam.first()) {
        let probes = PROBES.with(|cell| cell.borrow().as_ref().map(|(p, _)| p.clone()));
        if let Some(probes) = probes.filter(|p| !p.is_empty()) {
            let mut totals = Vec::with_capacity(probes.len());
            for probe in probes {
                totals.push(crate::scoring_v3::with_tuning(probe, || {
                    v3::Context::new(chart, c)?.total(best)
                })?);
            }
            PROBES.with(|cell| {
                if let Some((_, slot)) = cell.borrow_mut().as_mut() {
                    *slot = totals;
                }
            });
        }
    }
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
        let (used_bundle, used_carry, used_sweep, used_pair_press) = (
            state.used_bundle,
            state.used_carry,
            state.used_sweep,
            state.used_pair_press,
        );
        let mut skipped: Vec<SkippedPart> = given.to_vec();
        skipped.extend(state.dropped.to_vec().into_iter().map(|d| SkippedPart {
            note_id: chart.notes[d.note].id.clone(),
            part: if d.slide { "slide" } else { "contact" }.into(),
            time_seconds: d.time,
        }));
        skipped.sort_by(|a, b| {
            a.time_seconds
                .total_cmp(&b.time_seconds)
                .then_with(|| a.note_id.cmp(&b.note_id))
                .then_with(|| a.part.cmp(&b.part))
        });
        skipped.dedup();
        let [left, right] = state.arms;
        let mut warnings = vec![
            "單點與圓形手掌的 Demo 幾何近似，未模擬實機感測器判定或手臂關節。".into(),
            "Beam Search 不保證全域最優；交叉成本於候選完整後排序，搜尋剪枝依其他成本。".into(),
        ];
        if c.is_v2() {
            warnings[1] = "直覺優先 V2：搜尋已包含活動接觸交叉；Beam Search 不保證全域最佳，分數不是人類使用機率。".into();
        }
        if c.is_v3() {
            warnings[1] = "人類動作 V3：動作效率、跨區與短期負荷是 Demo 偏好；Beam Search 不保證全域最佳，不代表人體極限或機率。".into();
        }
        if c.is_v3() && c.slide_shortcut && chart.notes.iter().any(|n| n.path_id.is_some()) {
            warnings.push("Slide 依判定區抄近（MajdataPlay 判定佇列）：可跳過單一判定區、不必追到星星終點，手在最後判定區的正解時刻完成；感應區大小與指尖半徑是 Demo 近似。".into());
        }
        if chart.paths.iter().any(|path| path.branches.len() == 2) {
            warnings.push(if c.is_v3() && c.slide_shortcut {
                "WiFi 依三條判定佇列分工：一手張開覆蓋中央與一側、另一手覆蓋另一側，或單手張開覆蓋三條；手掌半徑沿用設定值。".into()
            } else {
                "WiFi 以雙手同步 2+1 覆蓋近似：一手沿中央與相鄰側線的中點移動，另一手沿剩餘側線；手掌半徑沿用設定值。".into()
            });
        }
        if used_bundle {
            warnings.push(if c.slide_shortcut {
                "部分同時出發的 Slide 由一隻手張開同時覆蓋兩條（掌心手掌半徑內的感應區都算碰到）。"
                    .into()
            } else {
                "部分同時出發的 Slide 由一隻手張開同時覆蓋兩條：掌心沿兩顆星星的中點畫到終點，兩顆星星都在手掌半徑內。"
                    .into()
            });
        }
        if used_carry {
            warnings.push(
                "部分星星重疊的 Slide 由同一隻手一起畫：後一條出發時，手追著的那條星星正好在它的起點，之後兩顆星星重合。".into(),
            );
        }
        if used_sweep {
            warnings.push(
                "部分 Slide 畫一段就放開，剩下的判定區由判定路線相同、較晚出發的 Slide 一筆畫過時一起完成。".into(),
            );
        }
        if used_pair_press {
            warnings.push("部分同時的相鄰兩鍵由一隻手張開一起按下（手落在兩鍵中間）。".into());
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
            score: if c.is_v3() {
                state.v3.score.map(SolutionScore::V3)
            } else {
                state.v2.score.map(SolutionScore::V2)
            },
            score_breakdown: if c.is_v3() {
                Some(SolutionScoreBreakdown::V3(state.v3.parts))
            } else if c.is_v2() {
                Some(SolutionScoreBreakdown::V2(state.v2.parts))
            } else {
                None
            },
            scoring_model: if !c.is_legacy() {
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
            skipped,
        });
        if solutions.len() == c.top_k {
            break;
        }
    }
    Ok(solutions)
}

/// 狀態指紋用的快速雜湊（rustc 的 FxHash）；只需確定、不需抗碰撞攻擊。
struct FxHasher(u64);
impl std::hash::Hasher for FxHasher {
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut word = [0u8; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            self.write_u64(u64::from_le_bytes(word));
        }
    }
    fn write_u64(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
    fn write_i64(&mut self, word: i64) {
        self.write_u64(word as u64);
    }
    fn write_usize(&mut self, word: usize) {
        self.write_u64(word as u64);
    }
    fn finish(&self) -> u64 {
        self.0
    }
}

/// 之後的發展只取決於這些欄位：兩手位置與進行中的動作段（含還會被改寫或順帶碰觸、
/// 交叉成本會回頭比對的近期段落）、進行中的 Slide／手掌／Touch Hold，以及 V3 的歷史。
/// 指紋相同的兩個狀態未來完全一樣，而 V3 成本可加，較貴的那個永遠不會勝出。
fn fingerprint(state: &State, time: f64) -> u64 {
    use std::hash::{Hash, Hasher};
    let q = |x: f64| (x * 1e6).round() as i64;
    let mut h = FxHasher(0);
    let horizon = state.arms[0].free.min(state.arms[1].free).min(time)
        - (TOUCH_LATE_FRAMES + 1.0) * JUDGE_FRAME;
    for arm in &state.arms {
        (
            q(arm.point.x),
            q(arm.point.y),
            q(arm.free),
            arm.last_tap.map(q),
        )
            .hash(&mut h);
        let mut cursor = arm.segments.head.as_ref();
        while let Some(link) = cursor {
            let segment = &link.value;
            (
                &segment.mode,
                &segment.note_id,
                q(segment.start_seconds),
                q(segment.end_seconds),
            )
                .hash(&mut h);
            for sample in &segment.samples {
                (q(sample.time_seconds), q(sample.x), q(sample.y)).hash(&mut h);
            }
            if segment.start_seconds < horizon {
                break;
            }
            cursor = link.prev.head.as_ref();
        }
        u8::MAX.hash(&mut h);
    }
    state
        .owners
        .iter()
        .for_each(|(n, hand)| (n, hand.index()).hash(&mut h));
    state
        .engaged
        .iter()
        .for_each(|(n, t)| (n, q(*t)).hash(&mut h));
    state.wifi_pairs.hash(&mut h);
    state.bundled.hash(&mut h);
    state.bundle_lead.hash(&mut h);
    state.carried.hash(&mut h);
    state.swept.hash(&mut h);
    state
        .committed
        .iter()
        .for_each(|(n, t)| (n, q(*t)).hash(&mut h));
    state.finished.hash(&mut h);
    state
        .suspended
        .iter()
        .for_each(|(n, (u, _))| (n, q(*u)).hash(&mut h));
    state.resumed.hash(&mut h);
    state
        .deposit
        .iter()
        .for_each(|(n, v)| (n, q(*v)).hash(&mut h));
    state
        .last_handover
        .iter()
        .for_each(|(n, t)| (n, q(*t)).hash(&mut h));
    q(state.pending).hash(&mut h);
    state.held_touch.hash(&mut h);
    for palm in &state.active_palms {
        palm.as_ref()
            .map(|p| {
                (
                    p.hand.index(),
                    q(p.center.x),
                    q(p.center.y),
                    q(p.radius),
                    q(p.start_seconds),
                    q(p.end_seconds),
                    &p.covered_note_ids,
                )
            })
            .hash(&mut h);
    }
    state.v3.fingerprint(&mut h);
    h.finish()
}

/// Preserve a few distinct active owner/deferred states within the same budget.
fn prune(states: &mut Vec<State>, c: &SolverConfig, time: f64) {
    states.sort_by(|a, b| compare_states(a, b, c, false));
    if c.is_v3() {
        // 合併未來完全等價的狀態，只留最便宜的；否則 beam 會被「過去不同、之後一樣」的
        // 複本塞滿（例如同一條 Slide 在不同 checkpoint 接上），真正不同的手順反而被剪掉。
        let mut seen = std::collections::HashSet::new();
        states.retain(|state| seen.insert(fingerprint(state, time)));
    }
    if c.is_legacy() || states.len() <= c.beam_width || c.beam_width < 4 {
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

#[allow(clippy::too_many_arguments)]
fn record_action(
    state: &mut State,
    c: &SolverConfig,
    hand: Hand,
    point: Point,
    time_seconds: f64,
    kind: ActionKind,
    note_id: Option<String>,
) {
    if c.is_v3() {
        state.actions = state.actions.push(ActionEvent {
            hand,
            point,
            time_seconds,
            kind,
            note_id,
        });
    }
}
fn compare_states(a: &State, b: &State, c: &SolverConfig, final_rank: bool) -> std::cmp::Ordering {
    match c.scoring_model.as_str() {
        "human-motion-v3" => v3::compare(a, b, final_rank),
        "hand-affinity-v2" => v2::compare(a, b, final_rank),
        "legacy-v1" => {
            if final_rank {
                a.cost.total().total_cmp(&b.cost.total())
            } else {
                a.rank().total_cmp(&b.rank())
            }
        }
        _ => unreachable!("validated scoring model"),
    }
}
