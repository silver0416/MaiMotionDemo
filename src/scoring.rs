//! V2 scoring primitives shared by incremental search and final verification.
//! Values are versioned Demo preferences, not physiological limits.

use crate::{Hand, MotionSample, MotionSegment, Point};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

pub const SCORING_MODEL: &str = "hand-affinity-v2";
const NEUTRAL_MARGIN: f64 = 0.15;
const RANK_PRECISION: f64 = 1e-6;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct PreferenceConfig {
    pub home_preference: f64,
    pub travel_comfort: f64,
    pub repeat_tolerance: f64,
    pub handover_willingness: f64,
}

impl Default for PreferenceConfig {
    fn default() -> Self {
        Self {
            home_preference: 80.0,
            travel_comfort: 30.0,
            repeat_tolerance: 60.0,
            handover_willingness: 40.0,
        }
    }
}

impl PreferenceConfig {
    pub fn validate(&self) -> Result<(), String> {
        for (name, value, min, max) in [
            ("homePreference", self.home_preference, 0.0, 100.0),
            ("travelComfort", self.travel_comfort, 1.0, 200.0),
            ("repeatTolerance", self.repeat_tolerance, 0.0, 100.0),
            ("handoverWillingness", self.handover_willingness, 0.0, 100.0),
        ] {
            if !value.is_finite() || !(min..=max).contains(&value) {
                return Err(format!("{name} 必須為 {min}–{max} 的有限數值"));
            }
        }
        Ok(())
    }
}

/// All fields are weighted contributions, except free_distance (screen radii).
/// The caller groups semantic contacts and deduplicates shared Slide tracking.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScoreBreakdown {
    pub assignment_affinity: f64,
    pub side_exposure: f64,
    pub cross_exposure: f64,
    pub handover: f64,
    pub travel_strain: f64,
    pub compression_strain: f64,
    pub repetition: f64,
    pub free_distance: f64,
}

/// Constructed through ScoringV2::score so the comparator never receives NaN.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Score {
    intuition: f64,
    strain: f64,
    efficiency: f64,
    ranking_value: f64,
}

impl Score {
    pub fn intuition(&self) -> f64 {
        self.intuition
    }
    pub fn strain(&self) -> f64 {
        self.strain
    }
    pub fn efficiency(&self) -> f64 {
        self.efficiency
    }
    pub fn ranking_value(&self) -> f64 {
        self.ranking_value
    }

    /// The solver preserves stable candidate order when all score keys tie.
    /// Quantizing each value separately preserves a transitive ordering.
    pub fn compare(&self, other: &Self) -> Ordering {
        (self.ranking_value / RANK_PRECISION)
            .round()
            .total_cmp(&(other.ranking_value / RANK_PRECISION).round())
            .then_with(|| self.efficiency.total_cmp(&other.efficiency))
            .then_with(|| self.ranking_value.total_cmp(&other.ranking_value))
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MotionTerms {
    /// Includes the fixed 0.25 coefficient.
    pub side_exposure: f64,
    pub travel_strain: f64,
    /// Diagnostic raw tracking burden, excluded from ranking by default.
    /// Solver must deduplicate handover overlap before comparing to baseline.
    pub tracking_strain: f64,
    pub free_distance: f64,
}

pub struct ScoringV2 {
    config: PreferenceConfig,
}

impl ScoringV2 {
    pub fn new(config: PreferenceConfig) -> Result<Self, String> {
        config.validate()?;
        Ok(Self { config })
    }

    pub fn config(&self) -> &PreferenceConfig {
        &self.config
    }

    pub fn strain_weight(&self) -> f64 {
        2_f64.powf(4.0 - 8.0 * self.config.home_preference / 100.0)
    }

    /// One actual contact decision; duration and checkpoint count are irrelevant.
    pub fn affinity(&self, hand: Hand, point: Point) -> Result<f64, String> {
        validate_point(point)?;
        let sign = if hand == Hand::L { 1.0 } else { -1.0 };
        checked(
            ((sign * point.x - NEUTRAL_MARGIN) / (1.0 - NEUTRAL_MARGIN))
                .max(0.0)
                .powi(2),
        )
    }

    /// Integrated one-sided Huber burden for a constant-speed piece.
    /// Uses distance/time algebra without constructing an unbounded speed.
    pub fn movement_strain(&self, distance: f64, seconds: f64) -> Result<f64, String> {
        checked(distance)?;
        checked(seconds)?;
        if seconds == 0.0 {
            return if distance == 0.0 {
                Ok(0.0)
            } else {
                Err("非零距離不能零秒移動".into())
            };
        }
        let scaled = distance / self.config.travel_comfort;
        let excess = (scaled - seconds).max(0.0);
        checked(if excess <= seconds {
            (excess / seconds) * excess
        } else {
            2.0 * excess - seconds
        })
    }

    /// The caller supplies only genuine restrikes, not Slide checkpoints or
    /// additions to a held palm. repetition_seconds remains an advanced setting.
    pub fn repetition(&self, gap: f64, repetition_seconds: f64) -> Result<f64, String> {
        checked(gap)?;
        if !repetition_seconds.is_finite() || repetition_seconds <= 0.0 {
            return Err("連打間隔必須為有限正數".into());
        }
        checked(
            2.0 * (1.0 - self.config.repeat_tolerance / 100.0)
                * (1.0 - gap / repetition_seconds).max(0.0).powi(2),
        )
    }

    pub fn handover(&self, meeting_swap: bool) -> f64 {
        if meeting_swap {
            0.0
        } else {
            0.1 + 0.9 * (1.0 - self.config.handover_willingness / 100.0)
        }
    }

    /// Inputs are totals over one entire Slide with overlap counted once.
    pub fn compression(&self, actual: f64, nominal: f64) -> Result<f64, String> {
        checked(actual)?;
        checked(nominal)?;
        checked((actual - nominal).max(0.0))
    }

    pub fn score(&self, parts: &ScoreBreakdown) -> Result<Score, String> {
        for value in [
            parts.assignment_affinity,
            parts.side_exposure,
            parts.cross_exposure,
            parts.handover,
            parts.travel_strain,
            parts.compression_strain,
            parts.repetition,
            parts.free_distance,
        ] {
            checked(value)?;
        }
        let intuition = checked(
            parts.assignment_affinity + parts.side_exposure + parts.cross_exposure + parts.handover,
        )?;
        let strain = checked(parts.travel_strain + parts.compression_strain + parts.repetition)?;
        let ranking_value = checked(intuition + self.strain_weight() * strain)?;
        // Keep the quantized key finite as well as the displayed score.
        checked(ranking_value / RANK_PRECISION)?;
        Ok(Score {
            intuition,
            strain,
            efficiency: parts.free_distance,
            ranking_value,
        })
    }

    /// A removable contribution: callers can replace glide/palm segments without
    /// leaving their old costs behind. Tap affinity is accounted separately.
    pub fn motion_terms(&self, segment: &MotionSegment, hand: Hand) -> Result<MotionTerms, String> {
        validate_segment(segment)?;
        let mut terms = MotionTerms::default();
        let sign = if hand == Hand::L { 1.0 } else { -1.0 };
        for pair in segment.samples.windows(2) {
            let dt = pair[1].time_seconds - pair[0].time_seconds;
            let distance = pair[0].point().distance(pair[1].point());
            let strain = self.movement_strain(distance, dt)?;
            if active(&segment.mode) && segment.mode != "tap" {
                terms.side_exposure += 0.25
                    * positive_square_integral(
                        (sign * pair[0].x - NEUTRAL_MARGIN) / (1.0 - NEUTRAL_MARGIN),
                        (sign * pair[1].x - NEUTRAL_MARGIN) / (1.0 - NEUTRAL_MARGIN),
                        dt,
                    );
            }
            match segment.mode.as_str() {
                "travel" | "glide" => {
                    terms.travel_strain += strain;
                    terms.free_distance += distance;
                }
                "slide" | "handover" => terms.tracking_strain += strain,
                _ => {}
            }
        }
        for value in [
            terms.side_exposure,
            terms.travel_strain,
            terms.tracking_strain,
            terms.free_distance,
        ] {
            checked(value)?;
        }
        Ok(terms)
    }

    /// Exact integral on shared sample intervals, never comparing endpoints at
    /// different times. Invoke on overlapping segment pairs in the dirty region.
    pub fn crossing(&self, left: &MotionSegment, right: &MotionSegment) -> Result<f64, String> {
        validate_segment(left)?;
        validate_segment(right)?;
        if !active(&left.mode) || !active(&right.mode) {
            return Ok(0.0);
        }
        let (mut i, mut j) = (0, 0);
        let mut cost = 0.0;
        while i + 1 < left.samples.len() && j + 1 < right.samples.len() {
            let (la, lb) = (&left.samples[i], &left.samples[i + 1]);
            let (ra, rb) = (&right.samples[j], &right.samples[j + 1]);
            let start = la.time_seconds.max(ra.time_seconds);
            let end = lb.time_seconds.min(rb.time_seconds);
            if end > start {
                cost += positive_square_integral(
                    (x_at(la, lb, start) - x_at(ra, rb, start)) / 2.0,
                    (x_at(la, lb, end) - x_at(ra, rb, end)) / 2.0,
                    end - start,
                );
            }
            if lb.time_seconds <= rb.time_seconds {
                i += 1;
            }
            if rb.time_seconds <= lb.time_seconds {
                j += 1;
            }
        }
        checked(cost)
    }
}

fn checked(value: f64) -> Result<f64, String> {
    if !value.is_finite() || value < 0.0 {
        Err("V2 成本與時間必須為有限非負值".into())
    } else {
        Ok(value)
    }
}

fn validate_point(point: Point) -> Result<(), String> {
    if !point.x.is_finite() || !point.y.is_finite() {
        Err("軌跡座標必須為有限值".into())
    } else {
        Ok(())
    }
}

fn active(mode: &str) -> bool {
    matches!(
        mode,
        "tap" | "hold" | "palm" | "slide" | "handover" | "glide"
    )
}

fn validate_segment(segment: &MotionSegment) -> Result<(), String> {
    if !active(&segment.mode) && !matches!(segment.mode.as_str(), "travel" | "idle") {
        return Err(format!("未知動作模式：{}", segment.mode));
    }
    if segment.samples.len() < 2
        || !segment.start_seconds.is_finite()
        || !segment.end_seconds.is_finite()
        || segment.end_seconds < segment.start_seconds
    {
        return Err("動作段需有有效起訖與至少兩個取樣點".into());
    }
    if segment.samples.first().unwrap().time_seconds != segment.start_seconds
        || segment.samples.last().unwrap().time_seconds != segment.end_seconds
    {
        return Err("動作段起訖必須符合取樣時間".into());
    }
    for sample in &segment.samples {
        validate_point(sample.point())?;
        if !sample.time_seconds.is_finite() {
            return Err("取樣時間必須有限".into());
        }
    }
    for pair in segment.samples.windows(2) {
        let dt = pair[1].time_seconds - pair[0].time_seconds;
        checked(dt)?;
        if dt == 0.0 && pair[0].point() != pair[1].point() {
            return Err("零秒取樣不能改變位置".into());
        }
    }
    Ok(())
}

fn x_at(a: &MotionSample, b: &MotionSample, time: f64) -> f64 {
    let u = (time - a.time_seconds) / (b.time_seconds - a.time_seconds);
    a.x * (1.0 - u) + b.x * u
}

/// Integral of max(0, lerp(a,b,t))² over dt. Split at the exact zero crossing.
fn positive_square_integral(a: f64, b: f64, dt: f64) -> f64 {
    if dt == 0.0 || (a <= 0.0 && b <= 0.0) {
        return 0.0;
    }
    if a >= 0.0 && b >= 0.0 {
        return dt * (a * a + a * b + b * b) / 3.0;
    }
    if a > 0.0 {
        dt * (a / (a - b)) * a * a / 3.0
    } else {
        dt * (b / (b - a)) * b * b / 3.0
    }
}
