//! 分組認知：依節奏把按鍵切成樂句。形狀（各組的相對鍵位與節奏，左右鏡像視為
//! 同一種）相同的樂句，玩家傾向用同一套手法；手法樣板由玩家標註學出
//! （examples/learn_templates.rs），出貨的表為 templates.json。
use super::*;
use std::sync::OnceLock;

/// 超過這個間隔一定分組。
const PHRASE_MAX_GAP: f64 = 0.4;
/// 間隔比前一個間隔長這麼多倍時分組。
const PHRASE_BREAK_RATIO: f64 = 1.6;
const PHRASE_MAX_NOTES: usize = 8;

/// 一個樂句：形狀簽名與依簽名順序排列的音符。
pub(crate) struct Phrase<'a> {
    pub signature: String,
    pub notes: Vec<&'a Note>,
}

/// 按鍵接觸（含 Slide 起點、和弦），依時間分組後切成樂句。
pub(crate) fn phrases(chart: &Chart) -> Vec<Phrase<'_>> {
    let mut contacts: Vec<&Note> = chart
        .notes
        .iter()
        .filter(|n| n.touch_area.is_none() && (n.kind != "slide" || n.has_head))
        .collect();
    contacts.sort_by(|a, b| {
        a.time_seconds
            .total_cmp(&b.time_seconds)
            .then(a.button.cmp(&b.button))
    });
    let mut groups: Vec<Vec<&Note>> = vec![];
    for note in contacts {
        match groups.last_mut() {
            Some(g) if (note.time_seconds - g[0].time_seconds).abs() < 0.002 => g.push(note),
            _ => groups.push(vec![note]),
        }
    }
    let mut result = vec![];
    let mut current: Vec<Vec<&Note>> = vec![];
    for group in groups {
        if let Some(last) = current.last() {
            let gap = group[0].time_seconds - last[0].time_seconds;
            let previous = (current.len() >= 2)
                .then(|| last[0].time_seconds - current[current.len() - 2][0].time_seconds);
            if gap > PHRASE_MAX_GAP || previous.is_some_and(|p| gap > PHRASE_BREAK_RATIO * p) {
                result.extend(phrase(&current));
                current.clear();
            }
        }
        current.push(group);
    }
    result.extend(phrase(&current));
    result
}

fn phrase<'a>(groups: &[Vec<&'a Note>]) -> Option<Phrase<'a>> {
    let count: usize = groups.iter().map(Vec::len).sum();
    if groups.len() < 2 || count > PHRASE_MAX_NOTES {
        return None;
    }
    let base = groups[0][0].button;
    let times: Vec<f64> = groups.iter().map(|g| g[0].time_seconds).collect();
    let first_gap = times[1] - times[0];
    let rhythm: Vec<String> = times
        .windows(2)
        .map(|w| ((w[1] - w[0]) / first_gap * 4.0).round().to_string())
        .collect();
    // 兩個方向各自排好組內順序，取字串較小者為標準方向。
    let oriented = |mirror: bool| {
        let mut notes = vec![];
        let mut parts = vec![];
        for group in groups {
            let mut rel: Vec<(u8, &Note)> = group
                .iter()
                .map(|n| {
                    let r = (n.button + 8 - base) % 8;
                    (if mirror { (8 - r) % 8 } else { r }, *n)
                })
                .collect();
            rel.sort_by_key(|(r, _)| *r);
            parts.push(
                rel.iter()
                    .map(|(r, _)| r.to_string())
                    .collect::<Vec<_>>()
                    .join("+"),
            );
            notes.extend(rel.into_iter().map(|(_, n)| n));
        }
        (parts.join(","), notes)
    };
    let (normal, normal_notes) = oriented(false);
    let (mirrored, mirrored_notes) = oriented(true);
    let (shape, notes) = if mirrored < normal {
        (mirrored, mirrored_notes)
    } else {
        (normal, normal_notes)
    };
    Some(Phrase {
        signature: format!("{shape}|{}", rhythm.join(",")),
        notes,
    })
}

/// 手法：第一顆的手記為 A，同手 A、另一手 B。
pub(crate) fn pattern(hands: &[Hand]) -> String {
    hands
        .iter()
        .map(|h| if *h == hands[0] { 'A' } else { 'B' })
        .collect()
}

static TEMPLATES: OnceLock<BTreeMap<String, String>> = OnceLock::new();

/// 換一張樣板表（評估用）；已經讀過時回傳 false。
pub fn set_templates(templates: BTreeMap<String, String>) -> bool {
    TEMPLATES.set(templates).is_ok()
}

pub(crate) fn templates() -> &'static BTreeMap<String, String> {
    TEMPLATES.get_or_init(|| {
        serde_json::from_str(include_str!("templates.json")).expect("templates.json 格式錯誤")
    })
}

/// 樂句的形狀簽名與音符 id（簽名順序），供學習樣板。
pub fn phrase_shapes(chart: &Chart) -> Vec<(String, Vec<String>)> {
    phrases(chart)
        .into_iter()
        .map(|p| (p.signature, p.notes.iter().map(|n| n.id.clone()).collect()))
        .collect()
}

thread_local! {
    /// 研究用：這個執行緒上的求解當成已知的分組（每組是音符 key）。
    static GIVEN: std::cell::RefCell<Option<Vec<Vec<String>>>> =
        const { std::cell::RefCell::new(None) };
}

/// 研究用：在 `f` 裡的求解把 `groups`（每組是音符 key，例如真人標的分組）當成已知分組，
/// 收組內手型成本（groupPattern）。只影響目前執行緒，結束後恢復原狀。
pub fn with_groups<R>(groups: Vec<Vec<String>>, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<Vec<Vec<String>>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            GIVEN.with(|slot| *slot.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(GIVEN.with(|slot| slot.replace(Some(groups))));
    f()
}

/// 一組音符該怎麼分手。規則來自真人分組標註（records.20261016 的 86 個整組標了手的組，
/// 83 組符合）：同一顆鍵反覆時整組同手或嚴格交替；三顆以上往同方向走相鄰鍵（階梯）時
/// 單手掃過；其他（相鄰兩顆、跳鍵、三角、來回）嚴格左右交替。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GroupRule {
    SameOrAlternate,
    Same,
    Alternate,
}

/// 給定的分組（沒呼叫 with_groups 時為 None），每組依 [`classify`] 整理。
pub(crate) fn given_groups(chart: &Chart) -> Option<Vec<(Vec<&Note>, GroupRule)>> {
    let groups = GIVEN.with(|slot| slot.borrow().clone())?;
    let by_key: BTreeMap<&str, &Note> = chart.notes.iter().map(|n| (n.key.as_str(), n)).collect();
    Some(
        groups
            .iter()
            .filter_map(|keys| {
                classify(
                    keys.iter()
                        .filter_map(|k| by_key.get(k.as_str()).copied())
                        .collect(),
                )
            })
            .collect(),
    )
}

/// 一組音符的分手規則：只看單獨落下的接觸（和弦、沒有星星頭的 Slide 不算），依時間排序；
/// 少於兩顆時為 None。
pub(crate) fn classify(notes: Vec<&Note>) -> Option<(Vec<&Note>, GroupRule)> {
    let mut notes: Vec<&Note> = notes
        .into_iter()
        .filter(|n| n.kind != "slide" || n.has_head)
        .collect();
    notes.sort_by(|a, b| {
        a.time_seconds
            .total_cmp(&b.time_seconds)
            .then(a.button.cmp(&b.button))
    });
    let alone = |i: usize| {
        let t = notes[i].time_seconds;
        notes
            .iter()
            .enumerate()
            .all(|(j, n)| j == i || (n.time_seconds - t).abs() >= 0.002)
    };
    let notes: Vec<&Note> = (0..notes.len())
        .filter(|&i| alone(i))
        .map(|i| notes[i])
        .collect();
    if notes.len() < 2 {
        return None;
    }
    let buttons: Vec<u8> = notes.iter().map(|n| n.button).collect();
    let steps: Vec<u8> = buttons.windows(2).map(|w| (w[1] + 8 - w[0]) % 8).collect();
    let rule = if notes.iter().any(|n| n.touch_area.is_some()) {
        GroupRule::Alternate
    } else if buttons.iter().all(|b| *b == buttons[0]) {
        GroupRule::SameOrAlternate
    } else if notes.len() >= 3 && (steps.iter().all(|s| *s == 1) || steps.iter().all(|s| *s == 7)) {
        GroupRule::Same
    } else {
        GroupRule::Alternate
    };
    Some((notes, rule))
}

/// 自動分組最少幾顆才算同鍵反覆或階梯。
const RUN_MIN: usize = 3;
/// 節奏段超過這麼多個落點時再往下切。
const GROUP_MAX_ONSETS: usize = 4;
/// 擬人切法：同鍵反覆後的那一下，最多隔反覆間隔的幾倍還算連打的最後一下（2 ＝ 空一拍）。
const ATTACH_MAX_RATIO: f64 = 2.0;

type Onset<'a> = Vec<&'a Note>;

/// 自動分組怎麼切長段（[`TuningV3::group_cut`](crate::scoring_v3::TuningV3::group_cut)）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GroupCut {
    /// 從段首起依拍點切；Temoto 模型訓練時用的切法，模型特徵一律用這個。
    Beats,
    /// 依手的動作切（擬人切法），見 [`motion_split`]。
    Motion,
}

impl GroupCut {
    pub(crate) fn of(tuning: &crate::scoring_v3::TuningV3) -> Self {
        if tuning.group_cut >= 0.5 {
            Self::Motion
        } else {
            Self::Beats
        }
    }
}

fn is_hold(onset: &Onset) -> bool {
    onset.len() == 1 && onset[0].kind == "hold"
}

/// 自動分組（對照 records.20261016 的真人分組調出來）：
/// 1. 同鍵反覆與階梯（連續三顆以上單獨落下的按鍵、節奏均勻、不接 Hold）自成一組；
///    擬人切法時，緊接 Hold（節奏沒放慢）的最後一顆留給 Hold：點完接著按是一個動作，
///    留下的不到三顆就不算反覆；
/// 2. 其餘依節奏切段：間隔超過 PHRASE_MAX_GAP，或比前一個或後一個間隔長 PHRASE_BREAK_RATIO 倍；
/// 3. 超過 GROUP_MAX_ONSETS 個落點的段：拍點切法有兩顆以上星星頭就在每顆星星頭前切，
///    否則從段首起依拍點切（16 分兩顆一組、三連音與 24 分三顆一組）；擬人切法見 [`motion_split`]；
/// 4. 擬人切法時，落單的 Tap 見 [`attach_to_repeats`]。
///
/// 回傳每組的音符（含和弦）；少於兩個落點的不算一組。
pub(crate) fn auto_groups(chart: &Chart, cut: GroupCut) -> Vec<Vec<&Note>> {
    let onsets = onsets(chart);
    let single = |o: &Onset| o.len() == 1 && o[0].touch_area.is_none();
    let time = |i: usize| onsets[i][0].time_seconds;
    let mut groups: Vec<Vec<Onset>> = vec![];
    // 留給 Hold 的反覆最後一顆（落點索引）。
    let mut bonded: Vec<usize> = vec![];
    let mut free = 0;
    let mut i = 0;
    while i < onsets.len() {
        let mut best = i;
        if single(&onsets[i]) {
            for step in [0, 1, 7] {
                let mut j = i;
                while j + 1 < onsets.len()
                    && single(&onsets[j + 1])
                    && onsets[j + 1][0].kind != "hold"
                {
                    let gap = time(j + 1) - time(j);
                    let first = time(i + 1) - time(i);
                    if gap > PHRASE_MAX_GAP
                        || gap > PHRASE_BREAK_RATIO * first
                        || first > PHRASE_BREAK_RATIO * gap
                        || (onsets[j + 1][0].button + 8 - onsets[j][0].button) % 8 != step
                    {
                        break;
                    }
                    j += 1;
                }
                if j + 1 - i >= RUN_MIN && j > best {
                    best = j;
                }
            }
        }
        if best > i {
            let end = best;
            if cut == GroupCut::Motion
                && onsets.get(best + 1).is_some_and(is_hold)
                && time(best + 1) - time(best) <= PHRASE_BREAK_RATIO * (time(best) - time(best - 1))
            {
                best -= 1;
            }
            if best + 1 - i >= RUN_MIN {
                rhythm_segments(&onsets[free..i], cut, &mut groups);
                groups.push(onsets[i..=best].to_vec());
                free = best + 1;
                if best < end {
                    bonded.push(end);
                }
            }
            i = end + 1;
        } else {
            i += 1;
        }
    }
    rhythm_segments(&onsets[free..], cut, &mut groups);
    if cut == GroupCut::Motion {
        groups = attach_to_repeats(&onsets, groups, &bonded);
    }
    groups
        .into_iter()
        .filter(|g| g.len() >= 2)
        .map(|g| g.into_iter().flatten().collect())
        .collect()
}

/// 擬人切法：落單的一顆 Tap 併進前一組。`groups` 依序涵蓋所有落點。
/// - 留給 Hold 的反覆最後一顆（`bonded`）卻沒和 Hold 分在一起（Hold 自己開了反覆，或節奏在兩者之間切開）時，
///   還給反覆，不讓它落單；
/// - 緊接在同鍵反覆（全是單獨落下的同一顆鍵）後面、同一顆鍵，前面的間隔不超過反覆間隔的
///   ATTACH_MAX_RATIO 倍（最多空一拍）、也不比後面長時，併進那組（連打的最後一下）；隔了休息就不併。
fn attach_to_repeats<'a>(
    onsets: &[Onset<'a>],
    groups: Vec<Vec<Onset<'a>>>,
    bonded: &[usize],
) -> Vec<Vec<Onset<'a>>> {
    let time = |i: usize| onsets[i][0].time_seconds;
    let mut out: Vec<Vec<Onset>> = vec![];
    let mut at = 0;
    for group in groups {
        let i = at;
        at += group.len();
        if let ([lone], Some(last)) = (group.as_slice(), out.last_mut()) {
            if bonded.contains(&i) {
                last.push(lone.clone());
                continue;
            }
            let repeats = last.len() >= 2
                && last.iter().all(|o| {
                    o.len() == 1 && o[0].touch_area.is_none() && o[0].button == lone[0].button
                });
            let before = time(i) - time(i - 1);
            let inner = match last.as_slice() {
                [.., a, b] => b[0].time_seconds - a[0].time_seconds,
                _ => 0.0,
            };
            let after = if i + 1 < onsets.len() {
                time(i + 1) - time(i)
            } else {
                f64::INFINITY
            };
            if repeats
                && lone.len() == 1
                && lone[0].kind == "tap"
                && before <= ATTACH_MAX_RATIO * inner + 1e-6
                && before <= after + 1e-6
            {
                last.push(lone.clone());
                continue;
            }
        }
        out.push(group);
    }
    out
}

/// 所有接觸（含星星頭，不含沒有星星頭的 Slide）依時間排序，同時落下的併成一個落點。
fn onsets(chart: &Chart) -> Vec<Onset<'_>> {
    let mut contacts: Vec<&Note> = chart
        .notes
        .iter()
        .filter(|n| n.kind != "slide" || n.has_head)
        .collect();
    contacts.sort_by(|a, b| {
        a.time_seconds
            .total_cmp(&b.time_seconds)
            .then(a.button.cmp(&b.button))
    });
    let mut onsets: Vec<Onset> = vec![];
    for note in contacts {
        match onsets.last_mut() {
            Some(o) if (note.time_seconds - o[0].time_seconds).abs() < 0.002 => o.push(note),
            _ => onsets.push(vec![note]),
        }
    }
    onsets
}

fn rhythm_segments<'a>(onsets: &[Onset<'a>], cut: GroupCut, groups: &mut Vec<Vec<Onset<'a>>>) {
    if onsets.is_empty() {
        return;
    }
    let gaps: Vec<f64> = onsets
        .windows(2)
        .map(|w| w[1][0].time_seconds - w[0][0].time_seconds)
        .collect();
    let mut start = 0;
    for (i, &gap) in gaps.iter().enumerate() {
        let longer = |other: Option<&f64>| other.is_some_and(|o| gap > PHRASE_BREAK_RATIO * o);
        if gap > PHRASE_MAX_GAP
            || longer(i.checked_sub(1).map(|k| &gaps[k]))
            || longer(gaps.get(i + 1))
        {
            split_segment(&onsets[start..=i], cut, groups);
            start = i + 1;
        }
    }
    split_segment(&onsets[start..], cut, groups);
}

fn split_segment<'a>(segment: &[Onset<'a>], cut: GroupCut, groups: &mut Vec<Vec<Onset<'a>>>) {
    if cut == GroupCut::Motion {
        // 三四顆的短段裡有 Hold（不在段首）也要看點完接 Hold 該不該切開。
        if segment.len() > GROUP_MAX_ONSETS
            || (segment.len() >= 3 && segment[1..].iter().any(is_hold))
        {
            motion_split(segment, groups);
        } else {
            groups.push(segment.to_vec());
        }
        return;
    }
    if segment.len() <= GROUP_MAX_ONSETS {
        groups.push(segment.to_vec());
        return;
    }
    let heads: Vec<usize> = (0..segment.len())
        .filter(|&i| segment[i].len() == 1 && segment[i][0].kind == "slide")
        .collect();
    if heads.len() < 2 {
        beat_slots(segment, groups);
        return;
    }
    let mut start = 0;
    for end in heads.into_iter().chain([segment.len()]) {
        if end == start {
            continue;
        }
        let part = &segment[start..end];
        if part.len() > GROUP_MAX_ONSETS {
            beat_slots(part, groups);
        } else {
            groups.push(part.to_vec());
        }
        start = end;
    }
}

/// 從段首起依拍點切：一格是 1/2、1 或 2 拍中第一個剛好容納兩個以上間隔的，都不合時是兩個間隔。
fn beat_slots<'a>(segment: &[Onset<'a>], groups: &mut Vec<Vec<Onset<'a>>>) {
    let mut gaps: Vec<f64> = segment
        .windows(2)
        .map(|w| w[1][0].beat - w[0][0].beat)
        .collect();
    gaps.sort_by(f64::total_cmp);
    let gap = gaps[gaps.len() / 2].max(1e-6);
    let slot = [0.5, 1.0, 2.0]
        .into_iter()
        .find(|s| {
            let n = s / gap;
            n >= 1.999 && (n - n.round()).abs() < 0.05
        })
        .unwrap_or(2.0 * gap);
    let origin = segment[0][0].beat;
    let mut current: Vec<Onset> = vec![];
    let mut index = None;
    for onset in segment {
        let k = ((onset[0].beat - origin + 1e-6) / slot).floor() as i64;
        if index != Some(k) && !current.is_empty() {
            groups.push(std::mem::take(&mut current));
        }
        index = Some(k);
        current.push(onset.clone());
    }
    groups.push(current);
}

/// 擬人切法的參數（對照 groups12 的真人分組調出來，2026-10-27；逐首留一驗證完全符合
/// 102/135 組，拍點切法 96/135）。
const MOTION_PEAK: f64 = 1.0;
const MOTION_HOLD_PAIR: f64 = 2.0;
const MOTION_GROUP: f64 = 2.0;
const MOTION_SINGLE: f64 = 3.0;
const MOTION_MAX_ONSETS: usize = 5;

/// 擬人切法：在手換動作的地方切。相鄰兩個落點之間切開的分數是手在環上要跳多遠（0–4 格，
/// 和弦取最近的一對，Touch 算 2），比前後兩處都遠時再加 MOTION_PEAK，切在「接 Hold 的那顆
/// Tap」前加 MOTION_HOLD_PAIR（點完接著按是一個動作）；每多一組扣 MOTION_GROUP，
/// 單一落點的組再扣 MOTION_SINGLE，每組最多 MOTION_MAX_ONSETS 個落點，取總分最高的切法
/// （同分取前面的組比較大的）。
fn motion_split<'a>(segment: &[Onset<'a>], groups: &mut Vec<Vec<Onset<'a>>>) {
    let m = segment.len();
    let jump: Vec<f64> = (0..m)
        .map(|k| {
            if k == 0 {
                0.0
            } else {
                ring_jump(&segment[k - 1], &segment[k])
            }
        })
        .collect();
    let cut: Vec<f64> = (0..m)
        .map(|k| {
            if k == 0 {
                return 0.0;
            }
            let left = if k > 1 { jump[k - 1] } else { -1.0 };
            let right = jump.get(k + 1).copied().unwrap_or(-1.0);
            let mut score = jump[k];
            if jump[k] > left && jump[k] > right {
                score += MOTION_PEAK;
            }
            if segment[k].len() == 1
                && segment[k][0].kind == "tap"
                && segment.get(k + 1).is_some_and(is_hold)
            {
                score += MOTION_HOLD_PAIR;
            }
            score
        })
        .collect();
    let mut best = vec![f64::NEG_INFINITY; m + 1];
    let mut back = vec![0; m + 1];
    best[0] = 0.0;
    for end in 1..=m {
        for start in end.saturating_sub(MOTION_MAX_ONSETS)..end {
            let mut score = best[start] + cut[start] - MOTION_GROUP;
            if end - start == 1 {
                score -= MOTION_SINGLE;
            }
            if score > best[end] {
                best[end] = score;
                back[end] = start;
            }
        }
    }
    let mut bounds = vec![m];
    while let Some(&end) = bounds.last().filter(|&&e| e > 0) {
        bounds.push(back[end]);
    }
    for w in bounds.windows(2).rev() {
        groups.push(segment[w[1]..w[0]].to_vec());
    }
}

/// 兩個落點之間手在環上要跳幾格（0–4）：和弦取最近的一對，有 Touch 的一對算 2。
fn ring_jump(a: &Onset, b: &Onset) -> f64 {
    let mut best = 4;
    for x in a {
        for y in b {
            let d = if x.touch_area.is_some() || y.touch_area.is_some() {
                2
            } else {
                let d = (y.button + 8 - x.button) % 8;
                d.min(8 - d)
            };
            best = best.min(d);
        }
    }
    f64::from(best)
}

/// 自動分組只在這些速度收手型成本（組內最長間隔，秒）：階梯不超過 AUTO_STAIR_GAP；
/// 相鄰兩顆的交替在 AUTO_PAIR_GAP 範圍內。更快的兩顆真人多半一手連打兩顆，
/// 更慢的、三顆以上的交替真人做法分散（eval18 與 records.20261016 的統計）。
const AUTO_STAIR_GAP: f64 = 0.12;
const AUTO_PAIR_GAP: (f64, f64) = (0.075, 0.12);
/// 超過這麼多顆的階梯（繞過半圈以上）真人多半一側一手，不要求單手。
const AUTO_STAIR_MAX: usize = 5;
/// 同鍵連打最長間隔不超過 AUTO_JACK_GAP 秒、至少 AUTO_JACK_MIN 顆時嚴格交替：四個標註集
/// （eval18、records.20261016、records.20261016.2份、records.20261024.8份）這樣的 25 串
/// 全都交替；間隔 0.11–0.25 秒的 47 串沒有一串交替。
const AUTO_JACK_GAP: f64 = 0.10;
const AUTO_JACK_MIN: usize = 5;

/// 自動分組與各組的分手規則，第三項是交替要接續的前一顆（不屬於這組，只用來比對手）。
/// 規則比給定分組保守：
/// - 快速同鍵連打（AUTO_JACK_GAP、AUTO_JACK_MIN）：嚴格交替；前一個落點是單獨一顆、
///   和連打第一顆的間隔也一樣快時，接續它的手（一路交替下來的連打）；
/// - 整組落在同一半（1–4 或 5–8），前後相鄰組（間隔不超過 PHRASE_MAX_GAP）都整組在另一半：
///   一側一手，整組同手；
/// - 其他同鍵反覆：整組同手或嚴格交替；
/// - 階梯：只在快、而且不超過 AUTO_STAIR_MAX 顆時要求單手；
/// - 兩顆不同鍵：只在 AUTO_PAIR_GAP 的速度要求一左一右；
/// - 其餘不收成本。
pub(crate) fn auto_rules(
    chart: &Chart,
    cut: GroupCut,
) -> Vec<(Vec<&Note>, GroupRule, Option<&Note>)> {
    let onsets = onsets(chart);
    let groups = auto_groups(chart, cut);
    let half = |g: &Vec<&Note>| -> Option<bool> {
        if g.iter().any(|n| n.touch_area.is_some()) {
            return None;
        }
        let right = g[0].button <= 4;
        g.iter().all(|n| (n.button <= 4) == right).then_some(right)
    };
    let start = |g: &Vec<&Note>| {
        g.iter()
            .map(|n| n.time_seconds)
            .fold(f64::INFINITY, f64::min)
    };
    let end = |g: &Vec<&Note>| {
        g.iter()
            .map(|n| n.time_seconds)
            .fold(f64::NEG_INFINITY, f64::max)
    };
    let mut result = vec![];
    for (i, group) in groups.iter().enumerate() {
        let Some((notes, rule)) = classify(group.clone()) else {
            continue;
        };
        let longest = notes
            .windows(2)
            .map(|w| w[1].time_seconds - w[0].time_seconds)
            .fold(0.0, f64::max);
        let opposite = |j: Option<usize>, near: bool| {
            near && half(group).is_some()
                && j.and_then(|j| groups.get(j)).and_then(half) == half(group).map(|r| !r)
        };
        let before = i.checked_sub(1);
        let after = Some(i + 1);
        let split = opposite(
            before,
            before.is_some_and(|j| start(group) - end(&groups[j]) <= PHRASE_MAX_GAP),
        ) && opposite(
            after,
            groups
                .get(i + 1)
                .is_some_and(|next| start(next) - end(group) <= PHRASE_MAX_GAP),
        );
        if rule == GroupRule::SameOrAlternate
            && longest <= AUTO_JACK_GAP
            && notes.len() >= AUTO_JACK_MIN
        {
            let first = notes[0].time_seconds;
            let lead = onsets
                .iter()
                .rev()
                .find(|o| o[0].time_seconds < first - 0.002)
                .filter(|o| o.len() == 1 && first - o[0].time_seconds <= AUTO_JACK_GAP)
                .map(|o| o[0]);
            result.push((notes, GroupRule::Alternate, lead));
            continue;
        }
        let rule = if split {
            Some(GroupRule::Same)
        } else {
            match rule {
                GroupRule::SameOrAlternate => Some(rule),
                GroupRule::Same => {
                    (longest <= AUTO_STAIR_GAP && notes.len() <= AUTO_STAIR_MAX).then_some(rule)
                }
                GroupRule::Alternate => {
                    (notes.len() == 2 && longest > AUTO_PAIR_GAP.0 && longest <= AUTO_PAIR_GAP.1)
                        .then_some(rule)
                }
            }
        };
        if let Some(rule) = rule {
            result.push((notes, rule, None));
        }
    }
    result
}

/// 自動分組的音符 key（評估用），切法依目前的 V3 權重（groupCut）。
pub fn auto_group_keys(chart: &Chart) -> Vec<Vec<String>> {
    group_keys(auto_groups(
        chart,
        GroupCut::of(crate::scoring_v3::tuning()),
    ))
}

/// 擬人切法的自動分組音符 key（不看 V3 權重）：app 標註預填分組用。依時間排序，組內依落點。
pub fn motion_group_keys(chart: &Chart) -> Vec<Vec<String>> {
    group_keys(auto_groups(chart, GroupCut::Motion))
}

fn group_keys(groups: Vec<Vec<&Note>>) -> Vec<Vec<String>> {
    groups
        .into_iter()
        .map(|g| g.into_iter().map(|n| n.key.clone()).collect())
        .collect()
}

/// 依規則數有幾顆的手不合：全同手看和第一顆不同的，嚴格交替看沒有輪流的。
pub(crate) fn group_deviation(rule: GroupRule, hands: &[Hand]) -> usize {
    let same = hands.iter().filter(|h| **h != hands[0]).count();
    let alternate = hands
        .iter()
        .enumerate()
        .filter(|(i, h)| (**h == hands[0]) != (i % 2 == 0))
        .count();
    match rule {
        GroupRule::Same => same,
        GroupRule::Alternate => alternate,
        GroupRule::SameOrAlternate => same.min(alternate),
    }
}
