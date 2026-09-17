//! V3 movement economy and short-term preferences. All constants are Demo
//! calibration defaults, not physiological measurements or official judgments.
use crate::scoring::{PreferenceConfig, ScoringV2};
use crate::{Hand, MotionSegment, Point};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

pub const SCORING_MODEL_V3: &str = "human-motion-v3";
const RANK_PRECISION: f64 = 1e-6;
const HOME_NEUTRAL_MARGIN: f64 = 0.15;
const HOME_EXPOSURE_RATE: f64 = 0.10;
const BASE_TRAVEL_WEIGHT: f64 = 0.18;
const REVERSAL_WEIGHT: f64 = 0.28;
const REVERSAL_WINDOW: f64 = 0.60;
const REVERSAL_MIN_DISTANCE: f64 = 0.12;
const REVERSAL_FULL_DISTANCE: f64 = 0.75;
const JACK_SAME_POINT_RADIUS: f64 = 0.12;
const JACK_MAX_WEIGHT: f64 = 1.20;
const WORKLOAD_TAU: f64 = 0.45;
const WORKLOAD_FREE_LEVEL: f64 = 1.25;
const WORKLOAD_WEIGHT: f64 = 0.12;

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
            travel_comfort: 90.0,
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
}
impl ScoreBreakdownV3 {
    pub fn values(&self) -> [f64; 9] {
        [
            self.travel,
            self.speed_strain,
            self.compression_strain,
            self.excursion,
            self.cross_exposure,
            self.handover,
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
    pub fn crossing(&self, left: &MotionSegment, right: &MotionSegment) -> Result<f64, String> {
        self.shared.crossing(left, right)
    }
    pub fn handover(&self, swap: bool) -> f64 {
        self.shared.handover(swap)
    }
    pub fn score(&self, p: &ScoreBreakdownV3) -> Result<ScoreV3, String> {
        for v in p.values() {
            checked(v)?;
        }
        let movement = checked(p.travel + p.speed_strain + p.compression_strain)?;
        let posture = checked(p.excursion + p.cross_exposure + p.handover)?;
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
        let mut entry = 0.;
        if s.mode != "idle" {
            for pair in s.samples.windows(2) {
                entry += (opposite_depth(hand, pair[1].point()).powi(2)
                    - opposite_depth(hand, pair[0].point()).powi(2))
                .max(0.);
            }
        }
        Ok(MotionTermsV3 {
            travel: BASE_TRAVEL_WEIGHT * shared.free_distance,
            speed_strain: shared.travel_strain,
            tracking_strain: shared.tracking_strain,
            excursion: checked(
                weight * (entry + HOME_EXPOSURE_RATE * shared.side_exposure / 0.25),
            )?,
        })
    }
    pub fn action(
        &self,
        h: &mut HandHistory,
        event: &ActionEvent,
        repetition_seconds: f64,
    ) -> Result<ScoreBreakdownV3, String> {
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
        let decayed = h.workload * (-dt / WORKLOAD_TAU).exp();
        let load = match event.kind {
            ActionKind::Strike => 1.,
            ActionKind::ContinuousContact => 0.65,
            ActionKind::Palm => 1.1,
        };
        // Only using the busy hand has a marginal cost. An idle hand is never penalized.
        let mut p = ScoreBreakdownV3 {
            workload: WORKLOAD_WEIGHT * (decayed - WORKLOAD_FREE_LEVEL).max(0.).powi(2) * load,
            ..Default::default()
        };
        if let (Some(prev), Some(last), Some(time)) = (h.prev_point, h.last_point, h.last_time) {
            p.reversal = reversal(prev, last, event.point, event.time_seconds - time)?;
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
            p.jack_fatigue = JACK_MAX_WEIGHT
                * (1. - self.config.jack_tolerance / 100.)
                * (h.jack_streak.saturating_sub(2) as f64).powf(1.5)
                * (1. - gap / repetition_seconds).clamp(0., 1.).powi(2);
            h.jack_point = Some(event.point);
            h.last_strike_time = Some(event.time_seconds);
        } else {
            // Continuous contact/palm interrupts a restrike run, never adds a jack.
            h.jack_point = None;
            h.last_strike_time = None;
            h.jack_streak = 0;
        }
        h.prev_point = h.last_point;
        h.last_point = Some(event.point);
        h.last_time = Some(event.time_seconds);
        h.workload = decayed + load;
        h.workload_time = Some(event.time_seconds);
        self.score(&p)?;
        Ok(p)
    }
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
        REVERSAL_WEIGHT
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
    jack_point: Option<Point>,
    jack_streak: u32,
    last_strike_time: Option<f64>,
    workload: f64,
    workload_time: Option<f64>,
}
