//! V3 movement economy and short-term preferences. All constants are Demo
//! calibration defaults, not physiological measurements or official judgments.
use crate::scoring::{PreferenceConfig, ScoringV2};
use crate::{Hand, MotionSegment, Point};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

pub const SCORING_MODEL_V3: &str = "human-motion-v3";
const RANK_PRECISION: f64 = 1e-6;
const HOME_NEUTRAL_MARGIN: f64 = 0.15;
const REVERSAL_WINDOW: f64 = 0.60;
const REVERSAL_MIN_DISTANCE: f64 = 0.12;
const REVERSAL_FULL_DISTANCE: f64 = 0.75;
const JACK_SAME_POINT_RADIUS: f64 = 0.12;
/// Same-key restrikes up to this streak are normal play, never jack fatigue.
const JACK_FREE_STREAK: u32 = 3;
// V3.1 short-term ownership. Internal Demo calibration, not UI parameters.
pub const OWNERSHIP_TAU: f64 = 0.8;
pub const OWNERSHIP_INITIAL: f64 = 0.5;
pub const OWNERSHIP_GAIN: f64 = 0.6;
pub const OWNERSHIP_MAX: f64 = 1.0;
pub const OWNERSHIP_MIN: f64 = 0.05;
/// Two targets closer than this can share a temporary local lane.
pub const LOCAL_CLUSTER_RADIUS: f64 = 1.0;
/// Returning to a live anchor after a finished excursion keeps this share of reversal.
pub const ANCHOR_REVERSAL_DISCOUNT: f64 = 0.2;

/// V3 preference weights. Defaults were fitted (coordinate search) to player
/// hand annotations: 9 charts plus the earlier speed_first_full references,
/// see examples/annotation_batch.rs. A tuning run may install different values
/// once per process before solving.
#[doc(hidden)]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct TuningV3 {
    pub travel: f64,
    pub speed_strain: f64,
    /// Per second while both hands are past the midline margin on the opposite
    /// side, scaled by the shallower of the two depths. Working together on one
    /// side is free; a brief swap is cheap, staying swapped accumulates.
    pub swapped_posture: f64,
    /// Multiplier on the shared active-contact crossing integral.
    pub contact_cross: f64,
    pub home_entry: f64,
    pub home_exposure: f64,
    pub reversal: f64,
    pub jack: f64,
    /// Sharing work between the hands comes from this density term, not from
    /// raw speed: a hand repeating at 16th spacing tires far faster than at 8th.
    pub workload: f64,
    pub workload_tau: f64,
    pub workload_free: f64,
    /// Load share of restriking the point the hand just struck.
    pub same_point_load: f64,
    pub same_point_window: f64,
    pub ownership_switch: f64,
    /// Cost of pulling a hand off a key it is about to play again.
    pub anchor_hold: f64,
    /// A chord with several buttons forces a split, so taking an owned key costs less.
    pub chord_switch_discount: f64,
    /// Tapping a star with one hand and sliding it with the other (posture).
    pub star_switch: f64,
    /// Changing hands in the middle of a stair: from the second of three or
    /// more consecutive single buttons stepping to the neighbour in one direction.
    pub stair_break: f64,
    /// Longest gap between stair steps.
    pub stair_window: f64,
    /// Steps faster than this gap cost proportionally more to break (0 = flat).
    pub stair_fast: f64,
    /// Share of the burst speed one hand would pay for a stair step that is
    /// added to breaking the stair there (1 = speed never favours switching).
    pub stair_speed: f64,
    /// Changing hands inside a Touch stroke (touches drawn in one direction).
    pub stroke_break: f64,
    /// Longest gap and step length inside a Touch stroke.
    pub stroke_window: f64,
    pub stroke_step: f64,
    /// A shape seen shortly before (same steps, possibly rotated or mirrored)
    /// played with a mixed hand pattern: neither the same hands nor all swapped.
    pub shape_mix: f64,
    /// Per note of a rhythm phrase whose hand pattern differs from the learned template.
    pub phrase_template: f64,
    /// Share of burst speed charged while gliding along the ring.
    pub glide_speed: f64,
    /// Rank-only lookahead bias; kept mild.
    pub future_role: f64,
}
impl Default for TuningV3 {
    fn default() -> Self {
        Self {
            travel: 0.0,
            speed_strain: 0.5,
            swapped_posture: 6.0,
            contact_cross: 180.0,
            home_entry: 0.8,
            home_exposure: 0.10,
            reversal: 0.28,
            jack: 0.45,
            workload: 0.0,
            workload_tau: 0.72,
            workload_free: 1.25,
            same_point_load: 0.1,
            same_point_window: 0.3,
            ownership_switch: 0.45,
            anchor_hold: 0.0,
            chord_switch_discount: 0.25,
            star_switch: 1.0,
            stair_break: 0.2,
            stair_window: 0.3,
            stair_fast: 0.0,
            stair_speed: 0.5,
            stroke_break: 1.0,
            stroke_window: 0.15,
            stroke_step: 0.55,
            shape_mix: 0.0,
            phrase_template: 0.5,
            glide_speed: 1.0,
            future_role: 0.15,
        }
    }
}
static TUNING: std::sync::OnceLock<TuningV3> = std::sync::OnceLock::new();
/// Installs tuning weights for this process; returns false once weights are
/// already fixed (set earlier or read by any V3 score).
#[doc(hidden)]
pub fn set_tuning(tuning: TuningV3) -> bool {
    TUNING.set(tuning).is_ok()
}
#[doc(hidden)]
pub fn tuning() -> &'static TuningV3 {
    TUNING.get_or_init(TuningV3::default)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct PreferenceConfigV3 {
    pub home_preference: f64,
    pub travel_comfort: f64,
    pub jack_tolerance: f64,
    pub handover_willingness: f64,
}
impl Default for PreferenceConfigV3 {
    fn default() -> Self {
        Self {
            home_preference: 60.0,
            travel_comfort: 5.2,
            jack_tolerance: 60.0,
            handover_willingness: 40.0,
        }
    }
}
impl PreferenceConfigV3 {
    pub fn validate(&self) -> Result<(), String> {
        for (name, value, min, max) in [
            ("homePreference", self.home_preference, 0., 100.),
            ("travelComfort", self.travel_comfort, 1., 200.),
            ("jackTolerance", self.jack_tolerance, 0., 100.),
            ("handoverWillingness", self.handover_willingness, 0., 100.),
        ] {
            if !value.is_finite() || !(min..=max).contains(&value) {
                return Err(format!("{name} 必須為 {min}–{max} 的有限數值"));
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScoreBreakdownV3 {
    pub travel: f64,
    pub speed_strain: f64,
    pub compression_strain: f64,
    pub excursion: f64,
    pub cross_exposure: f64,
    pub handover: f64,
    pub jack_fatigue: f64,
    pub reversal: f64,
    pub workload: f64,
    /// Changing the short-term owner of a key or leaving a nested local lane.
    pub ownership_switch: f64,
}
impl ScoreBreakdownV3 {
    pub fn values(&self) -> [f64; 10] {
        [
            self.travel,
            self.speed_strain,
            self.compression_strain,
            self.excursion,
            self.cross_exposure,
            self.handover,
            self.ownership_switch,
            self.jack_fatigue,
            self.reversal,
            self.workload,
        ]
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScoreV3 {
    movement: f64,
    posture: f64,
    fatigue: f64,
    total: f64,
}
impl ScoreV3 {
    pub fn movement(&self) -> f64 {
        self.movement
    }
    pub fn posture(&self) -> f64 {
        self.posture
    }
    pub fn fatigue(&self) -> f64 {
        self.fatigue
    }
    pub fn total(&self) -> f64 {
        self.total
    }
    pub fn compare(&self, other: &Self) -> Ordering {
        (self.total / RANK_PRECISION)
            .round()
            .total_cmp(&(other.total / RANK_PRECISION).round())
            .then_with(|| self.total.total_cmp(&other.total))
    }
}
#[derive(Default)]
pub struct MotionTermsV3 {
    pub travel: f64,
    pub speed_strain: f64,
    pub excursion: f64,
    pub tracking_strain: f64,
}
pub struct ScoringV3 {
    config: PreferenceConfigV3,
    shared: ScoringV2,
}
impl ScoringV3 {
    pub fn new(config: PreferenceConfigV3) -> Result<Self, String> {
        config.validate()?;
        let shared = ScoringV2::new(PreferenceConfig {
            travel_comfort: config.travel_comfort,
            handover_willingness: config.handover_willingness,
            ..Default::default()
        })?;
        Ok(Self { config, shared })
    }
    pub fn movement_strain(&self, distance: f64, seconds: f64) -> Result<f64, String> {
        self.shared.movement_strain(distance, seconds)
    }
    pub fn compression(&self, actual: f64, nominal: f64) -> Result<f64, String> {
        self.shared.compression(actual, nominal)
    }
    /// Active-contact crossing (shared with V2) plus sustained swapped posture:
    /// the time integral of min(L right-side depth, R left-side depth) over
    /// every overlapping piece, including travel and idle waiting.
    pub fn crossing(&self, left: &MotionSegment, right: &MotionSegment) -> Result<f64, String> {
        self.crossing_until(left, right, f64::INFINITY)
    }
    /// Instantaneous swapped-posture rate (per second) for two hand points.
    pub fn swapped_rate(&self, left: Point, right: Point) -> f64 {
        let weight = tuning().swapped_posture;
        weight * opposite_depth(Hand::L, left).min(opposite_depth(Hand::R, right))
    }
    /// `until`: the last visible demand ends here; resting after the chart is
    /// not a posture the player holds.
    pub fn crossing_until(
        &self,
        left: &MotionSegment,
        right: &MotionSegment,
        until: f64,
    ) -> Result<f64, String> {
        let shared = tuning().contact_cross * self.shared.crossing(left, right)?;
        let weight = tuning().swapped_posture;
        checked(shared + weight * swapped_integral(left, right, until)?)
    }
    pub fn handover(&self, swap: bool) -> f64 {
        self.shared.handover(swap)
    }
    pub fn score(&self, p: &ScoreBreakdownV3) -> Result<ScoreV3, String> {
        for v in p.values() {
            checked(v)?;
        }
        let movement = checked(p.travel + p.speed_strain + p.compression_strain)?;
        let posture = checked(p.excursion + p.cross_exposure + p.handover + p.ownership_switch)?;
        let fatigue = checked(p.jack_fatigue + p.reversal + p.workload)?;
        let total = checked(movement + posture + fatigue)?;
        checked(total / RANK_PRECISION)?;
        Ok(ScoreV3 {
            movement,
            posture,
            fatigue,
            total,
        })
    }
    pub fn motion_terms(&self, s: &MotionSegment, hand: Hand) -> Result<MotionTermsV3, String> {
        // Reuse validated speed and exact exposure integrals, without changing V2.
        let shared = self.shared.motion_terms(s, hand)?;
        let weight = self.config.home_preference / 100.;
        // Per distance, not per second: a very short burst is not cheaper
        // than a longer move at the same speed.
        let mut speed = 0.;
        if matches!(s.mode.as_str(), "travel" | "glide") {
            for pair in s.samples.windows(2) {
                let d = pair[0].point().distance(pair[1].point());
                let dt = pair[1].time_seconds - pair[0].time_seconds;
                if d > 0. {
                    if dt <= 0. {
                        return Err("非零距離不能零秒移動".into());
                    }
                    let z = (d / dt / self.config.travel_comfort - 1.).max(0.);
                    let psi = if z <= 1. { z * z } else { 2. * z - 1. };
                    let share = if s.mode == "glide" {
                        tuning().glide_speed
                    } else {
                        1.
                    };
                    speed += share * tuning().speed_strain * psi * d;
                }
            }
        }
        let mut entry = 0.;
        if s.mode != "idle" {
            for pair in s.samples.windows(2) {
                entry += (opposite_depth(hand, pair[1].point()).powi(2)
                    - opposite_depth(hand, pair[0].point()).powi(2))
                .max(0.);
            }
        }
        Ok(MotionTermsV3 {
            travel: tuning().travel * shared.free_distance,
            speed_strain: checked(speed)?,
            tracking_strain: shared.tracking_strain,
            excursion: checked(
                weight
                    * (tuning().home_entry * entry
                        + tuning().home_exposure * shared.side_exposure / 0.25),
            )?,
        })
    }
    pub fn action(
        &self,
        h: &mut HandHistory,
        event: &ActionEvent,
        repetition_seconds: f64,
    ) -> Result<ScoreBreakdownV3, String> {
        self.action_with(h, event, repetition_seconds, 1.)
    }
    /// `reversal_scale` lets the solver discount a return to a live local anchor.
    pub fn action_with(
        &self,
        h: &mut HandHistory,
        event: &ActionEvent,
        repetition_seconds: f64,
        reversal_scale: f64,
    ) -> Result<ScoreBreakdownV3, String> {
        checked(reversal_scale)?;
        if !event.time_seconds.is_finite()
            || !event.point.x.is_finite()
            || !event.point.y.is_finite()
            || !repetition_seconds.is_finite()
            || repetition_seconds <= 0.
        {
            return Err("V3 動作需有效時間、位置與連打間隔".into());
        }
        let dt = event.time_seconds - h.workload_time.unwrap_or(event.time_seconds);
        checked(dt)?;
        let t = tuning();
        let decayed = h.workload * (-dt / t.workload_tau).exp();
        // 剛打過的同一點再打（連打）幾乎不需要移動，負荷較輕。
        let same_point = h
            .last_point
            .is_some_and(|p| p.distance(event.point) <= JACK_SAME_POINT_RADIUS)
            && h.last_time
                .is_some_and(|last| event.time_seconds - last < t.same_point_window);
        let share = if same_point { t.same_point_load } else { 1. };
        let load = share
            * match event.kind {
                ActionKind::Strike => 1.,
                ActionKind::ContinuousContact => 0.65,
                ActionKind::Palm => 1.1,
            };
        // Only using the busy hand has a marginal cost. An idle hand is never penalized.
        let mut p = ScoreBreakdownV3 {
            workload: t.workload * (decayed - t.workload_free).max(0.).powi(2) * load,
            ..Default::default()
        };
        // 回頭的間隔從手到達目前這個鍵算起：在鍵上重打一陣子才離開，不算剛到就折返。
        if let (Some(prev), Some(last), Some(time)) =
            (h.prev_point, h.last_point, h.arrival_time.or(h.last_time))
        {
            p.reversal =
                reversal_scale * reversal(prev, last, event.point, event.time_seconds - time)?;
        }
        if event.kind == ActionKind::Strike {
            let gap = event.time_seconds
                - h.last_strike_time
                    .unwrap_or(event.time_seconds - repetition_seconds);
            let same = h
                .jack_point
                .is_some_and(|p| p.distance(event.point) <= JACK_SAME_POINT_RADIUS);
            h.jack_streak = if same && gap < repetition_seconds {
                h.jack_streak.saturating_add(1)
            } else {
                1
            };
            p.jack_fatigue = t.jack
                * (1. - self.config.jack_tolerance / 100.)
                * (h.jack_streak.saturating_sub(JACK_FREE_STREAK) as f64).powf(1.5)
                * (1. - gap / repetition_seconds).clamp(0., 1.).powi(2);
            h.jack_point = Some(event.point);
            h.last_strike_time = Some(event.time_seconds);
        } else {
            // Continuous contact/palm interrupts a restrike run, never adds a jack.
            h.jack_point = None;
            h.last_strike_time = None;
            h.jack_streak = 0;
        }
        // 同一個鍵重打不改變「從哪裡來」：3 → 2 → 2 → 3 仍是一次回頭，
        // 不因為中間的重打把來的方向洗掉。
        if h.last_point
            .is_none_or(|p| p.distance(event.point) > JACK_SAME_POINT_RADIUS)
        {
            h.prev_point = h.last_point;
            h.arrival_time = Some(event.time_seconds);
        }
        h.last_point = Some(event.point);
        h.last_time = Some(event.time_seconds);
        h.workload = decayed + load;
        h.workload_time = Some(event.time_seconds);
        self.score(&p)?;
        Ok(p)
    }
}
fn point_at(samples: &[crate::MotionSample], i: usize, t: f64) -> Point {
    let (a, b) = (&samples[i], &samples[i + 1]);
    let dt = b.time_seconds - a.time_seconds;
    if dt <= 0. {
        return b.point();
    }
    a.point()
        .lerp(b.point(), ((t - a.time_seconds) / dt).clamp(0., 1.))
}
fn swapped_integral(
    left: &MotionSegment,
    right: &MotionSegment,
    until: f64,
) -> Result<f64, String> {
    let (l, r) = (&left.samples, &right.samples);
    let (mut i, mut j) = (0, 0);
    let mut total = 0.;
    while i + 1 < l.len() && j + 1 < r.len() {
        let start = l[i].time_seconds.max(r[j].time_seconds);
        let end = l[i + 1].time_seconds.min(r[j + 1].time_seconds).min(until);
        if end > start {
            // Composite Simpson; depth is clamp-linear, so a few pieces suffice.
            const PIECES: usize = 4;
            let h = (end - start) / PIECES as f64;
            let f = |t: f64| {
                opposite_depth(Hand::L, point_at(l, i, t))
                    .min(opposite_depth(Hand::R, point_at(r, j, t)))
            };
            for k in 0..PIECES {
                let a = start + h * k as f64;
                total += h / 6. * (f(a) + 4. * f(a + h / 2.) + f(a + h));
            }
        }
        let (le, re) = (l[i + 1].time_seconds, r[j + 1].time_seconds);
        if le <= re {
            i += 1;
        }
        if re <= le {
            j += 1;
        }
    }
    checked(total)
}
fn opposite_depth(hand: Hand, p: Point) -> f64 {
    let sign = if hand == Hand::L { 1. } else { -1. };
    ((sign * p.x - HOME_NEUTRAL_MARGIN) / (1. - HOME_NEUTRAL_MARGIN)).clamp(0., 1.)
}
fn checked(v: f64) -> Result<f64, String> {
    if v.is_finite() && v >= 0. {
        Ok(v)
    } else {
        Err("V3 成本與間隔必須為有限非負值".into())
    }
}
// Dot products make reversal invariant under rotations; no special button patterns.
pub fn reversal(prev: Point, last: Point, current: Point, gap: f64) -> Result<f64, String> {
    checked(gap)?;
    let d1 = prev.distance(last);
    let d2 = last.distance(current);
    checked(d1)?;
    checked(d2)?;
    if d1 < REVERSAL_MIN_DISTANCE || d2 < REVERSAL_MIN_DISTANCE {
        return Ok(0.);
    }
    let cosine = ((last.x - prev.x) * (current.x - last.x)
        + (last.y - prev.y) * (current.y - last.y))
        / (d1 * d2);
    checked(
        tuning().reversal
            * (-cosine.clamp(-1., 1.)).max(0.).powi(2)
            * (1. - gap / REVERSAL_WINDOW).clamp(0., 1.).powi(2)
            * (d1.min(d2) / REVERSAL_FULL_DISTANCE).clamp(0., 1.),
    )
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionKind {
    Strike,
    ContinuousContact,
    Palm,
}
#[derive(Clone, Debug)]
pub struct ActionEvent {
    pub time_seconds: f64,
    pub point: Point,
    pub hand: Hand,
    pub kind: ActionKind,
    pub note_id: Option<String>,
}
#[derive(Clone, Debug, Default)]
pub struct HandHistory {
    prev_point: Option<Point>,
    last_point: Option<Point>,
    last_time: Option<f64>,
    /// 手到達 last_point 的時間；在同一個鍵上重打不會更新。
    arrival_time: Option<f64>,
    jack_point: Option<Point>,
    jack_streak: u32,
    last_strike_time: Option<f64>,
    workload: f64,
    workload_time: Option<f64>,
}
/// 狀態指紋用的量化：1e-6 以內視為相同。
fn quantize(x: f64) -> i64 {
    (x * 1e6).round() as i64
}
fn hash_point<H: std::hash::Hasher>(p: Option<Point>, h: &mut H) {
    use std::hash::Hash;
    p.map(|p| (quantize(p.x), quantize(p.y))).hash(h);
}
impl HandHistory {
    /// 影響之後成本的全部欄位（Beam 合併等價狀態用）。
    pub fn fingerprint<H: std::hash::Hasher>(&self, h: &mut H) {
        use std::hash::Hash;
        hash_point(self.prev_point, h);
        hash_point(self.last_point, h);
        self.last_time.map(quantize).hash(h);
        self.arrival_time.map(quantize).hash(h);
        hash_point(self.jack_point, h);
        self.jack_streak.hash(h);
        self.last_strike_time.map(quantize).hash(h);
        quantize(self.workload).hash(h);
        self.workload_time.map(quantize).hash(h);
    }
    /// (point before the last action, last action point).
    pub fn recent_points(&self) -> (Option<Point>, Option<Point>) {
        (self.prev_point, self.last_point)
    }
}

/// Discrete contact identity. V3.1 ownership only tracks outer buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ContactKey {
    Button(u8),
    Touch(char, u8),
}
impl std::fmt::Display for ContactKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Button(b) => write!(f, "{b}"),
            Self::Touch(area, index) => write!(f, "{area}{index}"),
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct KeyOwnership {
    pub owner: Hand,
    pub strength: f64,
    pub last_seen: f64,
}
impl KeyOwnership {
    pub fn strength_at(&self, time: f64) -> f64 {
        let s = self.strength * (-(time - self.last_seen).max(0.) / OWNERSHIP_TAU).exp();
        if s < OWNERSHIP_MIN {
            0.
        } else {
            s
        }
    }
}
/// Local, decaying owner per button. A fixed array keeps branch cloning
/// allocation-free and iteration deterministic.
#[derive(Clone, Debug, Default)]
pub struct OwnershipState {
    buttons: [Option<KeyOwnership>; 8],
}
impl OwnershipState {
    fn slot(key: ContactKey) -> Option<usize> {
        match key {
            ContactKey::Button(b @ 1..=8) => Some(b as usize - 1),
            _ => None,
        }
    }
    /// Owner and decayed strength; `None` once the ownership has faded.
    pub fn get(&self, key: ContactKey, time: f64) -> Option<(Hand, f64)> {
        let entry = self.buttons[Self::slot(key)?]?;
        let s = entry.strength_at(time);
        (s > 0.).then_some((entry.owner, s))
    }
    pub fn decay_to(&mut self, time: f64) {
        for slot in &mut self.buttons {
            if slot.is_some_and(|e| e.strength_at(time) == 0.) {
                *slot = None;
            }
        }
    }
    pub fn switch_cost(&self, key: ContactKey, hand: Hand, time: f64) -> f64 {
        match self.get(key, time) {
            Some((owner, s)) if owner != hand => tuning().ownership_switch * s,
            _ => 0.,
        }
    }
    pub fn observe(&mut self, key: ContactKey, hand: Hand, time: f64) {
        let Some(i) = Self::slot(key) else {
            return;
        };
        let strength = match self.get(key, time) {
            Some((owner, s)) if owner == hand => (s + OWNERSHIP_GAIN).min(OWNERSHIP_MAX),
            _ => OWNERSHIP_INITIAL,
        };
        self.buttons[i] = Some(KeyOwnership {
            owner: hand,
            strength,
            last_seen: time,
        });
    }
    pub fn entries(&self) -> impl Iterator<Item = (ContactKey, KeyOwnership)> + '_ {
        self.buttons
            .iter()
            .enumerate()
            .filter_map(|(i, e)| e.map(|e| (ContactKey::Button(i as u8 + 1), e)))
    }
}

/// A hand's temporary local anchor: a key it struck that recurs soon.
/// Never a lock; it expires at the anchor's next visit.
#[derive(Clone, Copy, Debug, Default)]
pub struct TemporaryRole {
    pub anchor: Option<ContactKey>,
    pub point: Point,
    pub strength: f64,
    pub expires_at: f64,
}
impl TemporaryRole {
    pub fn live(&self, time: f64) -> bool {
        self.anchor.is_some() && time <= self.expires_at + 1e-9
    }
}
/// Short-term ownership plus one temporary role per hand.
#[derive(Clone, Debug, Default)]
pub struct RoleState {
    pub ownership: OwnershipState,
    pub roles: [TemporaryRole; 2],
}
impl RoleState {
    /// 影響之後成本的全部欄位（Beam 合併等價狀態用）。
    pub fn fingerprint<H: std::hash::Hasher>(&self, h: &mut H) {
        use std::hash::Hash;
        for entry in &self.ownership.buttons {
            entry
                .map(|e| (e.owner.index(), quantize(e.strength), quantize(e.last_seen)))
                .hash(h);
        }
        for role in &self.roles {
            role.anchor.hash(h);
            hash_point(Some(role.point), h);
            quantize(role.strength).hash(h);
            quantize(role.expires_at).hash(h);
        }
    }
    /// Cost of a genuine contact before observing it: taking a key from its
    /// short-term owner, or abandoning an anchor that is revisited later than
    /// the other hand's anchor for a target inside that tighter local lane.
    /// `chord`: the chart demands several buttons at this instant.
    pub fn contact_cost(
        &self,
        hand: Hand,
        point: Point,
        key: ContactKey,
        time: f64,
        chord: bool,
    ) -> f64 {
        let mut cost = self.ownership.switch_cost(key, hand, time);
        if chord {
            cost *= tuning().chord_switch_discount;
        }
        let own = self.roles[hand.index()];
        let other = self.roles[1 - hand.index()];
        if own.live(time) && own.anchor != Some(key) && other.anchor != Some(key) {
            // Leaving a key this hand is about to play again. With both hands
            // anchored only the hand whose anchor waits longer gives way, and
            // then only inside the other hand's tighter local lane.
            let nested = other.live(time)
                && other.expires_at < own.expires_at - 1e-9
                && other.point.distance(point) <= LOCAL_CLUSTER_RADIUS;
            if !other.live(time) || nested {
                cost += tuning().anchor_hold * own.strength;
            }
        }
        cost
    }
    /// Reversal share for a return to this hand's live anchor after an
    /// excursion target that is not needed again soon.
    pub fn reversal_scale(
        &self,
        history: &HandHistory,
        hand: Hand,
        point: Point,
        time: f64,
        excursion_recurs: bool,
    ) -> f64 {
        let role = self.roles[hand.index()];
        let near = |p: Point| p.distance(role.point) <= JACK_SAME_POINT_RADIUS;
        let (prev, last) = history.recent_points();
        if role.live(time)
            && !excursion_recurs
            && near(point)
            && prev.is_some_and(near)
            && last.is_some_and(|p| !near(p))
        {
            ANCHOR_REVERSAL_DISCOUNT
        } else {
            1.
        }
    }
    /// `revisit`: the same key's next visit inside the lookahead window.
    pub fn observe(
        &mut self,
        hand: Hand,
        point: Point,
        key: ContactKey,
        revisit: Option<f64>,
        time: f64,
    ) {
        self.ownership.observe(key, hand, time);
        self.ownership.decay_to(time);
        for role in &mut self.roles {
            if !role.live(time) {
                *role = TemporaryRole::default();
            }
        }
        let strength = self.ownership.get(key, time).map_or(0., |(_, s)| s);
        let role = &mut self.roles[hand.index()];
        match revisit {
            // A shorter repeat nested inside the live anchor's lane keeps the outer anchor.
            Some(expires_at)
                if role.live(time)
                    && role.anchor != Some(key)
                    && expires_at < role.expires_at - 1e-9 => {}
            Some(expires_at) => {
                *role = TemporaryRole {
                    anchor: Some(key),
                    point,
                    strength,
                    expires_at,
                }
            }
            // Back on the anchor with no further visit: the phrase is over.
            None if role.anchor == Some(key) => *role = TemporaryRole::default(),
            None => {}
        }
    }
    /// A palm or other untracked contact replaces this hand's local plan.
    pub fn release(&mut self, hand: Hand) {
        self.roles[hand.index()] = TemporaryRole::default();
    }
}
