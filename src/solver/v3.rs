//! Incremental V3 bookkeeping around the existing feasibility transitions.
use super::*;
use crate::scoring_v3::{
    HandHistory, RoleState, ScoreBreakdownV3 as ScoreBreakdown, ScoreV3 as Score, ScoringV3,
};
pub(super) mod lookahead;
pub(super) mod phrases;
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
    /// Stair or Touch stroke step → the previous step's note and the cost of
    /// changing hands there.
    stair_prev: BTreeMap<&'a str, (&'a str, f64)>,
    /// 極短間隔的相鄰單顆接觸（sweepBreak）→ 前一顆與換手成本。
    sweep_prev: BTreeMap<&'a str, (&'a str, f64)>,
    /// 兩顆按鍵同時的和弦（chordCross）→ 另一顆。
    chord_pair: BTreeMap<&'a str, &'a str>,
    /// Last note of a repeated shape → its notes and the earlier occurrence's.
    shape_prev: BTreeMap<&'a str, ShapeRepeat<'a>>,
    /// Note of a phrase's last group → the phrase's notes and learned hand pattern.
    phrase_end: BTreeMap<&'a str, usize>,
    /// 樂句的音符與手法樣板；沒有樣板的樂句只看是不是整句單手（phraseOneHand）。
    phrase_templates: Vec<(Vec<&'a str>, Option<&'static str>)>,
    /// 分組（研究用的給定分組 phrases::with_groups，或 groupAuto 開啟時的自動分組）的音符
    /// → (組號, 組內位置)。
    group_at: BTreeMap<&'a str, (usize, usize)>,
    /// 分組的音符（依時間，交替要接續的前一顆排在最前面）與分手規則。
    groups: Vec<(Vec<&'a str>, phrases::GroupRule)>,
    /// 組內手型每顆不合的成本：給定分組用 groupPattern，自動分組用 groupAuto。
    group_weight: f64,
    /// End of the last note; swapped posture is not charged after it.
    posture_end: f64,
}

fn error(message: String) -> Diagnostic {
    Diagnostic::plain("invalid_score", message)
}

impl<'a> Context<'a> {
    pub fn new(chart: &'a Chart, c: &SolverConfig) -> Result<Self, Diagnostic> {
        let engine = ScoringV3::new(c.preferences_v3()).map_err(error)?;
        let mut phrase_end = BTreeMap::new();
        let mut phrase_templates = vec![];
        let one_hand = crate::scoring_v3::tuning().phrase_one_hand > 0.0;
        for phrase in phrases::phrases(chart) {
            let template = phrases::templates()
                .get(&phrase.signature)
                .map(String::as_str);
            let mut times: Vec<f64> = phrase.notes.iter().map(|n| n.time_seconds).collect();
            times.dedup_by(|a, b| (*a - *b).abs() < 0.002);
            if template.is_none() && !(one_hand && times.len() >= 3) {
                continue;
            }
            let last = phrase
                .notes
                .iter()
                .map(|n| n.time_seconds)
                .fold(f64::NEG_INFINITY, f64::max);
            for note in &phrase.notes {
                if (note.time_seconds - last).abs() < 0.002 {
                    phrase_end.insert(note.id.as_str(), phrase_templates.len());
                }
            }
            phrase_templates.push((
                phrase.notes.iter().map(|n| n.id.as_str()).collect(),
                template,
            ));
        }
        let tuning = crate::scoring_v3::tuning();
        let (found, group_weight) = match phrases::given_groups(chart) {
            Some(given) => (
                given.into_iter().map(|(n, r)| (n, r, None)).collect(),
                tuning.group_pattern,
            ),
            None if tuning.group_auto > 0.0 => (
                phrases::auto_rules(chart, phrases::GroupCut::of(tuning)),
                tuning.group_auto,
            ),
            None => (vec![], 0.0),
        };
        let mut group_at = BTreeMap::new();
        let mut groups = vec![];
        for (notes, rule, lead) in found {
            // 要接續的前一顆排在第 0 位：只用來比對手，不屬於這組，接觸它時不結算這組。
            let ids: Vec<&str> = lead.iter().chain(&notes).map(|n| n.id.as_str()).collect();
            for (position, id) in ids.iter().enumerate().skip(usize::from(lead.is_some())) {
                group_at.entry(*id).or_insert((groups.len(), position));
            }
            groups.push((ids, rule));
        }
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
            stair_prev: stair_steps(chart, c.travel_comfort),
            sweep_prev: sweep_steps(chart),
            chord_pair: chord_pairs(chart),
            shape_prev: shape_repeats(chart),
            phrase_end,
            phrase_templates,
            group_at,
            groups,
            group_weight,
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
            fresh.push(link);
            cursor = &link.prev;
        }
        fresh.reverse();
        let star: f64 = fresh
            .iter()
            .map(|link| {
                self.star_switch(link)
                    + self.stair_break(link)
                    + self.sweep_break(link)
                    + self.chord_cross(link)
                    + self.touch_palm(link)
                    + self.shape_mix(link)
                    + self.phrase_template(link)
                    + self.group_pattern(link)
            })
            .sum();
        next.v3.parts.ownership_switch += star;
        for link in fresh {
            let assignment = &link.value;
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

    /// 星星由一手拍下、第一次接上滑行的卻是另一手。
    /// （上下分工見檔尾的 split_upper。）只看這顆 Slide 的第一筆滑行；
    /// 往回找同一顆的起點，最多到起點時刻前 1 秒或 256 筆。
    fn star_switch(&self, link: &Link<Assignment>) -> f64 {
        let weight = crate::scoring_v3::tuning().star_switch;
        let slide = &link.value;
        if weight == 0.0 || slide.part != "slide" {
            return 0.0;
        }
        let note = self.notes[slide.note_id.as_str()];
        if !note.has_head {
            return 0.0;
        }
        let mut cursor = &link.prev;
        for _ in 0..256 {
            let Some(prev) = cursor.head.as_ref() else {
                break;
            };
            let a = &prev.value;
            if a.end_seconds < note.time_seconds - 1.0 {
                break;
            }
            if a.note_id == slide.note_id {
                match a.part.as_str() {
                    "slide" => return 0.0,
                    "head" if a.hand != slide.hand => return weight,
                    "head" => return 0.0,
                    _ => {}
                }
            }
            cursor = &prev.prev;
        }
        0.0
    }

    /// 樂句的手法與學到的樣板不同：依簽名順序比較，每顆不同收一次。在樂句最後一組
    /// 最晚指派的那顆結算（之前的還找不齊全部的手）。
    fn phrase_template(&self, link: &Link<Assignment>) -> f64 {
        let tuning = crate::scoring_v3::tuning();
        let current = &link.value;
        if (tuning.phrase_template == 0.0 && tuning.phrase_one_hand == 0.0)
            || !matches!(current.part.as_str(), "contact" | "head")
        {
            return 0.0;
        }
        let Some(&index) = self.phrase_end.get(current.note_id.as_str()) else {
            return 0.0;
        };
        let (notes, template) = &self.phrase_templates[index];
        let mut hands: Vec<Option<Hand>> = notes
            .iter()
            .map(|id| (*id == current.note_id).then_some(current.hand))
            .collect();
        let since = self.notes[notes[0]].time_seconds - 1.0;
        let mut cursor = &link.prev;
        for _ in 0..1024 {
            if hands.iter().all(Option::is_some) {
                break;
            }
            let Some(prev) = cursor.head.as_ref() else {
                break;
            };
            let a = &prev.value;
            if a.end_seconds < since {
                break;
            }
            if matches!(a.part.as_str(), "contact" | "head") {
                if let Some(i) = notes.iter().position(|id| *id == a.note_id) {
                    if a.note_id == current.note_id {
                        // 這顆之前已經接觸過：不是第一筆，不重複結算。
                        return 0.0;
                    }
                    hands[i].get_or_insert(a.hand);
                }
            }
            cursor = &prev.prev;
        }
        let Some(hands) = hands.into_iter().collect::<Option<Vec<Hand>>>() else {
            return 0.0;
        };
        let actual = phrases::pattern(&hands);
        match template {
            Some(template) => {
                let differing = actual
                    .chars()
                    .zip(template.chars())
                    .filter(|(a, b)| a != b)
                    .count();
                tuning.phrase_template * differing as f64
            }
            // 沒見過的形狀：一組快速的音符，人多半兩手合作，整句單手才收費。
            None if !actual.contains('B') => tuning.phrase_one_hand,
            None => 0.0,
        }
    }

    /// 分組的手型不合規則（phrases::GroupRule）：組內每顆第一次接觸時，收這顆讓
    /// 不合的顆數增加的部分，整組結算等於不合的顆數，搜尋途中就看得到。
    fn group_pattern(&self, link: &Link<Assignment>) -> f64 {
        let weight = self.group_weight;
        let current = &link.value;
        if weight == 0.0 || !matches!(current.part.as_str(), "contact" | "head") {
            return 0.0;
        }
        let Some(&(index, position)) = self.group_at.get(current.note_id.as_str()) else {
            return 0.0;
        };
        let (notes, rule) = &self.groups[index];
        let mut hands: Vec<Option<Hand>> = vec![None; position + 1];
        hands[position] = Some(current.hand);
        let since = self.notes[notes[0]].time_seconds - 1.0;
        let mut cursor = &link.prev;
        for _ in 0..4096 {
            let Some(prev) = cursor.head.as_ref() else {
                break;
            };
            let a = &prev.value;
            if a.end_seconds < since {
                break;
            }
            if matches!(a.part.as_str(), "contact" | "head") {
                if a.note_id == current.note_id {
                    // 這顆之前已經接觸過：不是第一筆，不重複結算。
                    return 0.0;
                }
                if let Some(i) = notes[..position].iter().position(|id| *id == a.note_id) {
                    hands[i].get_or_insert(a.hand);
                }
            }
            if hands.iter().all(Option::is_some) {
                break;
            }
            cursor = &prev.prev;
        }
        if position == 0 {
            return 0.0;
        }
        let Some(hands) = hands.into_iter().collect::<Option<Vec<Hand>>>() else {
            return 0.0;
        };
        let added = phrases::group_deviation(*rule, &hands)
            - phrases::group_deviation(*rule, &hands[..position]);
        weight * added as f64
    }

    /// 重複的形狀用了混合的手法：與前一次相比，既不是每顆同手，也不是每顆換手。
    /// 在形狀最後一顆的第一筆接觸時結算，往回找兩組音符的接觸手。
    fn shape_mix(&self, link: &Link<Assignment>) -> f64 {
        let weight = crate::scoring_v3::tuning().shape_mix;
        let current = &link.value;
        if weight == 0.0 || !matches!(current.part.as_str(), "contact" | "head") {
            return 0.0;
        }
        let Some(repeat) = self.shape_prev.get(current.note_id.as_str()) else {
            return 0.0;
        };
        let wanted: Vec<&str> = repeat.now.iter().chain(&repeat.before).copied().collect();
        let mut hands: Vec<Option<Hand>> = vec![None; wanted.len()];
        hands[repeat.now.len() - 1] = Some(current.hand);
        let since = self.notes[repeat.before[0]].time_seconds - 1.0;
        let mut cursor = &link.prev;
        for _ in 0..4096 {
            let Some(prev) = cursor.head.as_ref() else {
                break;
            };
            let a = &prev.value;
            if a.end_seconds < since {
                break;
            }
            if matches!(a.part.as_str(), "contact" | "head") {
                for (i, id) in wanted.iter().enumerate() {
                    if a.note_id == *id && i != repeat.now.len() - 1 {
                        hands[i] = Some(a.hand);
                    }
                }
                if hands.iter().all(Option::is_some) {
                    break;
                }
            }
            cursor = &prev.prev;
        }
        let Some(hands) = hands.into_iter().collect::<Option<Vec<Hand>>>() else {
            return 0.0;
        };
        let (now, before) = hands.split_at(repeat.now.len());
        let same = now.iter().zip(before).all(|(a, b)| a == b);
        let swapped = now.iter().zip(before).all(|(a, b)| a != b);
        if same || swapped {
            0.0
        } else {
            weight
        }
    }

    /// 階梯或 Touch 一筆畫的中途換手：這一步與上一步由不同的手接觸。只看這顆的
    /// 第一筆接觸，往回找上一步的接觸，最多到上一步前 1 秒或 256 筆。
    fn stair_break(&self, link: &Link<Assignment>) -> f64 {
        self.pair_break(link, &self.stair_prev)
    }

    /// 極短間隔的相鄰接觸中途換手（sweepBreak），算法同 stair_break。
    fn sweep_break(&self, link: &Link<Assignment>) -> f64 {
        self.pair_break(link, &self.sweep_prev)
    }

    fn pair_break(
        &self,
        link: &Link<Assignment>,
        pairs: &BTreeMap<&'a str, (&'a str, f64)>,
    ) -> f64 {
        let current = &link.value;
        if !matches!(current.part.as_str(), "contact" | "head") {
            return 0.0;
        }
        let Some(&(previous, weight)) = pairs.get(current.note_id.as_str()) else {
            return 0.0;
        };
        if weight == 0.0 {
            return 0.0;
        }
        match self.first_contact(link, previous, self.notes[previous].time_seconds - 1.0) {
            Some(hand) if hand != current.hand => weight,
            _ => 0.0,
        }
    }

    /// 往回找 `note` 的第一筆接觸的手，最多到 `since` 或 256 筆；先遇到目前這顆
    /// 自己較早的接觸（不是第一筆）就不算。
    fn first_contact(&self, link: &Link<Assignment>, note: &str, since: f64) -> Option<Hand> {
        let current = &link.value;
        let mut cursor = &link.prev;
        for _ in 0..256 {
            let prev = cursor.head.as_ref()?;
            let a = &prev.value;
            if a.end_seconds < since {
                return None;
            }
            if matches!(a.part.as_str(), "contact" | "head") {
                if a.note_id == current.note_id {
                    return None;
                }
                if a.note_id == note {
                    return Some(a.hand);
                }
            }
            cursor = &prev.prev;
        }
        None
    }

    /// 兩顆按鍵同時、兩手各一（chordCross）：左手在右手右邊算交叉；兩顆在同一側、
    /// 左右位置一樣時，從另一側來幫忙的手打上面那顆也算。後接觸的那顆結算。
    fn chord_cross(&self, link: &Link<Assignment>) -> f64 {
        let weight = crate::scoring_v3::tuning().chord_cross;
        let current = &link.value;
        if weight == 0.0 || !matches!(current.part.as_str(), "contact" | "head") {
            return 0.0;
        }
        let Some(&other) = self.chord_pair.get(current.note_id.as_str()) else {
            return 0.0;
        };
        let note = self.notes[current.note_id.as_str()];
        let Some(hand) = self.first_contact(link, other, note.time_seconds - 1.0) else {
            return 0.0;
        };
        if hand == current.hand {
            return 0.0;
        }
        let (left, right) = match current.hand {
            Hand::L => (note.position, self.notes[other].position),
            Hand::R => (self.notes[other].position, note.position),
        };
        if left.x > right.x + CHORD_SIDE {
            return weight;
        }
        if (left.x - right.x).abs() <= CHORD_SIDE && left.x.abs() > CHORD_SIDE {
            // 右側時左手是幫忙的手；y 小＝上面
            let (helper, owner) = if left.x > 0.0 {
                (left, right)
            } else {
                (right, left)
            };
            if helper.y < owner.y - EPS {
                return weight;
            }
        }
        0.0
    }

    /// 一隻手掌同時蓋住的 Touch 太散（touchPalm）：同一次手掌的接觸在路線上連續、
    /// 同一隻手、同一個開始時刻；依序加入時，最小包圍圓半徑第一次超過
    /// touchPalmFree 的那顆結算。
    fn touch_palm(&self, link: &Link<Assignment>) -> f64 {
        let t = crate::scoring_v3::tuning();
        let current = &link.value;
        let touch = |a: &Assignment| {
            a.part == "contact"
                && matches!(
                    self.notes[a.note_id.as_str()].kind.as_str(),
                    "touch" | "touchHold"
                )
        };
        if t.touch_palm == 0.0 || !touch(current) {
            return 0.0;
        }
        let mut points = vec![self.notes[current.note_id.as_str()].position];
        let mut cursor = &link.prev;
        while let Some(prev) = cursor.head.as_ref() {
            let a = &prev.value;
            if a.hand != current.hand
                || (a.start_seconds - current.start_seconds).abs() > EPS
                || !touch(a)
            {
                break;
            }
            points.push(self.notes[a.note_id.as_str()].position);
            cursor = &prev.prev;
        }
        let free = t.touch_palm_free + EPS;
        if enclosing_radius(&points) > free && enclosing_radius(&points[1..]) <= free {
            t.touch_palm
        } else {
            0.0
        }
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
        let split = if key.is_some() {
            split_upper(hand, point, time, &histories[1 - hand.index()])
        } else {
            0.0
        };
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
        terms.ownership_switch = switch + split;
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

    /// 暫停中的 Slide 若在 from 立刻接回、用剩下的時間追完剩下路徑的壓縮成本。
    /// 越晚接回只會越貴，所以是排序用的樂觀下限：暫停越早、之後越要趕路，
    /// 在排序上立刻看得到，beam 不會因為暫停暫時便宜就剪掉「滑到一半才放開」的狀態。
    pub fn resume_bound(
        &self,
        task: &Task,
        chart: &Chart,
        progress: f64,
        from: f64,
    ) -> Result<f64, Diagnostic> {
        let note = &chart.notes[task.note];
        let end = note.motion_end.unwrap();
        if from >= end - EPS || progress >= 1.0 - EPS {
            return Ok(0.0);
        }
        let samples = path_samples_range(&chart.paths[task.path], progress, 1.0, from, end);
        let distance: f64 = samples
            .windows(2)
            .map(|p| p[0].point().distance(p[1].point()))
            .sum();
        let segment = MotionSegment {
            mode: "slide".into(),
            note_id: Some(note.id.clone()),
            start_seconds: from,
            end_seconds: end,
            samples,
        };
        // 追蹤負擔只看速度，與哪一隻手無關。
        let terms = self.engine.motion_terms(&segment, Hand::L).map_err(error)?;
        let rate = self.nominal[note.id.as_str()];
        self.engine
            .compression(terms.tracking_strain, rate * distance)
            .map_err(error)
    }

    /// Independent complete trajectory recomputation for the final candidates.
    /// Replays physical actions and handovers as well as removable motion terms,
    /// so missing/duplicated incremental updates fail before publishing a score.
    pub fn verify(&self, state: &State) -> Result<(), Diagnostic> {
        let expected = self.recompute(state)?;
        let actual = &state.v3.parts;
        for (a, b) in expected.values().into_iter().zip(actual.values()) {
            if (a - b).abs() > 1e-7 * (1.0 + a.abs()) {
                return Err(error(format!("V3 增量成本不一致：{a} / {b}")));
            }
        }
        Ok(())
    }

    /// 學權重用：以目前的權重重算這條路線的總成本。
    pub fn total(&self, state: &State) -> Result<f64, Diagnostic> {
        let parts = self.recompute(state)?;
        Ok(self.engine.score(&parts).map_err(error)?.total())
    }

    /// 從完整路線重算各項成本（不靠增量）。
    fn recompute(&self, state: &State) -> Result<ScoreBreakdown, Diagnostic> {
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
        let mut cursor = &state.assignments;
        while let Some(link) = cursor.head.as_ref() {
            expected.ownership_switch += self.star_switch(link)
                + self.stair_break(link)
                + self.sweep_break(link)
                + self.chord_cross(link)
                + self.touch_palm(link)
                + self.shape_mix(link)
                + self.phrase_template(link)
                + self.group_pattern(link);
            cursor = &link.prev;
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
        Ok(expected)
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
    // 暫停中的 Slide 也有預存，但不是「晚接上」：手已經離開路線去做別的事，
    // 和同一組接觸位置相同的持續追蹤狀態並不等價，不能當成多餘的延後剔除。
    states.retain(|s| s.deposit.is_empty() || !s.suspended.is_empty() || !ready.contains(&key(s)));
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

/// 連續的單顆接觸，一路往同一個方向前進：
/// - 階梯：按鍵每一步走到同方向的相鄰鍵，間隔在 stairWindow 內。
/// - Touch 一筆畫：Touch 每一步距離在 strokeStep 內、間隔在 strokeWindow 內，
///   移動方向與上一步夾角小於 90°。
///
/// 整段至少三顆時，第二顆起每一步都算。階梯的換手成本另加單手走這一步的
/// 高速負擔 × stairSpeed，並在間隔短於 stairFast 時依比例放大。
///
/// 回傳每個算進去的步 → 上一步的音符與換手成本。
fn stair_steps(chart: &Chart, comfort: f64) -> BTreeMap<&str, (&str, f64)> {
    let t = crate::scoring_v3::tuning();
    let singles = single_contacts(chart);
    let touch = |n: &Note| n.touch_area.is_some();
    let button_step = |a: &Note, b: &Note| match (b.button + 8 - a.button) % 8 {
        1 => 1,
        7 => -1,
        _ => 0,
    };
    let mut result = BTreeMap::new();
    // 目前這一段：(音符, 按鍵方向 或 Touch 位移)
    let mut run: Vec<&Note> = vec![];
    let mut direction = 0;
    fn flush<'a>(
        run: &[&'a Note],
        weights: (f64, f64, f64, f64, f64),
        result: &mut BTreeMap<&'a str, (&'a str, f64)>,
    ) {
        if run.len() < 3 {
            return;
        }
        let (stair, stroke, fast, speed, comfort) = weights;
        let touch = run[0].touch_area.is_some();
        for pair in run.windows(2) {
            let gap = pair[1].time_seconds - pair[0].time_seconds;
            let weight = if touch {
                stroke
            } else {
                // 單手走這一步的高速負擔（與 motion_terms 相同的公式）。
                let d = pair[0].position.distance(pair[1].position);
                let z = (d / gap / comfort - 1.0).max(0.0);
                let burst = if z <= 1.0 { z * z } else { 2.0 * z - 1.0 } * d;
                let scale = if fast > 0.0 {
                    (fast / gap).max(1.0)
                } else {
                    1.0
                };
                stair * scale + speed * burst
            };
            result.insert(pair[1].id.as_str(), (pair[0].id.as_str(), weight));
        }
    }
    let weights = (
        t.stair_break,
        t.stroke_break,
        t.stair_fast,
        t.stair_speed * t.speed_strain,
        comfort,
    );
    for note in singles {
        let Some(note) = note else {
            flush(&run, weights, &mut result);
            run.clear();
            continue;
        };
        let joined = run.last().is_some_and(|&last| {
            if touch(last) != touch(note) {
                return false;
            }
            let gap = note.time_seconds - last.time_seconds;
            if touch(note) {
                let d = last.position.distance(note.position);
                let turn_ok = run.len() < 2 || {
                    let p = run[run.len() - 2].position;
                    (last.position.x - p.x) * (note.position.x - last.position.x)
                        + (last.position.y - p.y) * (note.position.y - last.position.y)
                        > 0.0
                };
                gap < t.stroke_window && d > 1e-9 && d <= t.stroke_step && turn_ok
            } else {
                let step = button_step(last, note);
                gap < t.stair_window && step != 0 && (run.len() < 2 || step == direction)
            }
        });
        if !joined {
            flush(&run, weights, &mut result);
            run.clear();
            direction = 0;
        } else if !touch(note) {
            direction = button_step(run[run.len() - 1], note);
        }
        run.push(note);
    }
    flush(&run, weights, &mut result);
    result
}

/// 單顆接觸（同時刻只有一顆）依時間排序；同時刻有兩顆以上的那個時刻記為 None。
fn single_contacts(chart: &Chart) -> Vec<Option<&Note>> {
    let mut contacts: Vec<&Note> = chart
        .notes
        .iter()
        .filter(|n| n.kind != "slide" || n.has_head)
        .collect();
    contacts.sort_by(|a, b| a.time_seconds.total_cmp(&b.time_seconds));
    let mut singles: Vec<Option<&Note>> = vec![];
    let mut last_time = f64::NEG_INFINITY;
    for note in contacts {
        if (note.time_seconds - last_time).abs() < 0.002 {
            if let Some(last) = singles.last_mut() {
                *last = None;
            }
        } else {
            singles.push(Some(note));
        }
        last_time = note.time_seconds;
    }
    singles
}

/// sweepBreak：相鄰兩顆單顆接觸（按鍵或 Touch）間隔不超過 sweepWindow、距離不超過
/// sweepStep 時，後一顆 → (前一顆, 換手成本)。標記者在這種地方幾乎都用同一隻手
/// （例如 tap 6 → E6 → B5 往內刷）。
fn sweep_steps(chart: &Chart) -> BTreeMap<&str, (&str, f64)> {
    let t = crate::scoring_v3::tuning();
    let mut result = BTreeMap::new();
    if t.sweep_break == 0.0 {
        return result;
    }
    for pair in single_contacts(chart).windows(2) {
        let (Some(a), Some(b)) = (pair[0], pair[1]) else {
            continue;
        };
        let gap = b.time_seconds - a.time_seconds;
        if gap <= t.sweep_window && a.position.distance(b.position) <= t.sweep_step {
            result.insert(b.id.as_str(), (a.id.as_str(), t.sweep_break));
        }
    }
    result
}

/// 和弦兩顆的左右位置差在這以內算「一樣」；位置離中線在這以內不算哪一側。
const CHORD_SIDE: f64 = 0.05;

/// chordCross：剛好兩顆按鍵（Tap、Hold、有頭的 Slide）同時的時刻，兩顆互相對應。
fn chord_pairs(chart: &Chart) -> BTreeMap<&str, &str> {
    let mut result = BTreeMap::new();
    if crate::scoring_v3::tuning().chord_cross == 0.0 {
        return result;
    }
    let mut heads: Vec<&Note> = chart
        .notes
        .iter()
        .filter(|n| n.kind != "slide" || n.has_head)
        .collect();
    heads.sort_by(|a, b| a.time_seconds.total_cmp(&b.time_seconds));
    for group in heads.chunk_by(|a, b| (b.time_seconds - a.time_seconds).abs() < 0.002) {
        if let [a, b] = group {
            if a.touch_area.is_none() && b.touch_area.is_none() && a.button != b.button {
                result.insert(a.id.as_str(), b.id.as_str());
                result.insert(b.id.as_str(), a.id.as_str());
            }
        }
    }
    result
}

/// 點的最小包圍圓半徑（點很少：試每兩點的直徑圓與每三點的外接圓）。
fn enclosing_radius(points: &[Point]) -> f64 {
    let reach = |center: Point| {
        points
            .iter()
            .map(|p| p.distance(center))
            .fold(0.0, f64::max)
    };
    let mut best = if points.len() < 2 { 0.0 } else { f64::INFINITY };
    for (i, &a) in points.iter().enumerate() {
        for (j, &b) in points.iter().enumerate().skip(i + 1) {
            best = best.min(reach(a.lerp(b, 0.5)));
            for &c in &points[j + 1..] {
                if let Some(center) = circumcenter(a, b, c) {
                    best = best.min(reach(center));
                }
            }
        }
    }
    best
}

const SHAPE_LENGTH: usize = 4;
const SHAPE_MAX_GAP: f64 = 0.35;
const SHAPE_LOOKBACK: f64 = 8.0;

/// 一個重複出現的形狀：這次與前一次的音符（逐顆對應）。
struct ShapeRepeat<'a> {
    now: [&'a str; SHAPE_LENGTH],
    before: [&'a str; SHAPE_LENGTH],
}

/// 連續 SHAPE_LENGTH 顆單顆按鍵、間隔不超過 SHAPE_MAX_GAP，步伐（相鄰兩鍵的鍵位差）
/// 與節奏（間隔比例）和 SHAPE_LOOKBACK 秒內較早、不重疊的一組相同，或左右鏡像，
/// 可以整體旋轉。每組只對應最近的一次。
fn shape_repeats(chart: &Chart) -> BTreeMap<&str, ShapeRepeat<'_>> {
    let mut contacts: Vec<&Note> = chart
        .notes
        .iter()
        .filter(|n| n.touch_area.is_none() && (n.kind != "slide" || n.has_head))
        .collect();
    contacts.sort_by(|a, b| a.time_seconds.total_cmp(&b.time_seconds));
    let mut singles: Vec<Option<&Note>> = vec![];
    let mut last_time = f64::NEG_INFINITY;
    for note in contacts {
        if (note.time_seconds - last_time).abs() < 0.002 {
            if let Some(last) = singles.last_mut() {
                *last = None;
            }
        } else {
            singles.push(Some(note));
        }
        last_time = note.time_seconds;
    }
    struct Window<'a> {
        start: usize,
        notes: [&'a Note; SHAPE_LENGTH],
        steps: [u8; SHAPE_LENGTH - 1],
        rhythm: [i64; SHAPE_LENGTH - 1],
    }
    let mut windows: Vec<Window> = vec![];
    for start in 0..singles.len().saturating_sub(SHAPE_LENGTH - 1) {
        let Some(notes) = singles[start..start + SHAPE_LENGTH]
            .iter()
            .copied()
            .collect::<Option<Vec<&Note>>>()
        else {
            continue;
        };
        let gaps: Vec<f64> = notes
            .windows(2)
            .map(|p| p[1].time_seconds - p[0].time_seconds)
            .collect();
        if gaps.iter().any(|g| *g > SHAPE_MAX_GAP) {
            continue;
        }
        let mut steps = [0; SHAPE_LENGTH - 1];
        let mut rhythm = [0; SHAPE_LENGTH - 1];
        for i in 0..SHAPE_LENGTH - 1 {
            steps[i] = (notes[i + 1].button + 8 - notes[i].button) % 8;
            rhythm[i] = (gaps[i] / gaps[0] * 4.0).round() as i64;
        }
        windows.push(Window {
            start,
            notes: notes.try_into().unwrap(),
            steps,
            rhythm,
        });
    }
    let mut result = BTreeMap::new();
    for (a, now) in windows.iter().enumerate() {
        let mirrored = now.steps.map(|s| (8 - s) % 8);
        let earlier = windows[..a].iter().rev().find(|before| {
            before.start + SHAPE_LENGTH <= now.start
                && now.notes[0].time_seconds - before.notes[0].time_seconds <= SHAPE_LOOKBACK
                && before.rhythm == now.rhythm
                && (before.steps == now.steps || before.steps == mirrored)
        });
        let Some(before) = earlier else {
            continue;
        };
        result.insert(
            now.notes[SHAPE_LENGTH - 1].id.as_str(),
            ShapeRepeat {
                now: now.notes.map(|n| n.id.as_str()),
                before: before.notes.map(|n| n.id.as_str()),
            },
        );
    }
    result
}

/// 兩手在短時間內都在同一半邊：這半邊的主人手守上面、來幫忙的手從下面進來。
/// 幫忙的手打在主人手上方時，依高度差收費，隨間隔在 splitWindow 內遞減。
fn split_upper(hand: Hand, point: Point, time: f64, other: &HandHistory) -> f64 {
    let tuning = crate::scoring_v3::tuning();
    if tuning.split_upper == 0.0 {
        return 0.0;
    }
    let Some((other_point, other_time)) = other.last_action() else {
        return 0.0;
    };
    let gap = time - other_time;
    const SIDE_MARGIN: f64 = 0.15;
    if !(0.0..tuning.split_window).contains(&gap)
        || point.x * other_point.x <= 0.0
        || point.x.abs() < SIDE_MARGIN
        || other_point.x.abs() < SIDE_MARGIN
    {
        return 0.0;
    }
    let owner = if point.x > 0.0 { Hand::R } else { Hand::L };
    let (owner_point, helper_point) = if hand == owner {
        (point, other_point)
    } else {
        (other_point, point)
    };
    // y 向下增加：幫忙的手 y 比較小就是在上方。
    tuning.split_upper
        * (owner_point.y - helper_point.y).max(0.0)
        * (1.0 - gap / tuning.split_window)
}
