use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
impl Point {
    pub fn distance(self, other: Self) -> f64 {
        (self.x - other.x).hypot(self.y - other.y)
    }
    pub fn lerp(self, other: Self, u: f64) -> Self {
        Self {
            x: self.x + (other.x - self.x) * u,
            y: self.y + (other.y - self.y) * u,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
    pub severity: String,
    pub source_span: Option<Box<SourceSpan>>,
    pub note_ids: Vec<String>,
    pub time_seconds: Option<f64>,
}
impl Diagnostic {
    pub fn plain(code: &str, message: String) -> Self {
        Self {
            code: code.into(),
            message,
            severity: "error".into(),
            source_span: None,
            note_ids: vec![],
            time_seconds: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathSample {
    pub u: f64,
    pub x: f64,
    pub y: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlidePath {
    pub id: String,
    pub samples: Vec<PathSample>,
}
impl SlidePath {
    pub fn at(&self, u: f64) -> Point {
        let u = u.clamp(0.0, 1.0);
        let i = self
            .samples
            .partition_point(|p| p.u < u)
            .clamp(1, self.samples.len() - 1);
        let (a, b) = (&self.samples[i - 1], &self.samples[i]);
        Point { x: a.x, y: a.y }.lerp(Point { x: b.x, y: b.y }, (u - a.u) / (b.u - a.u))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: String,
    pub kind: String,
    pub button: u8,
    pub time_seconds: f64,
    pub end_seconds: f64,
    pub position: Point,
    pub path_id: Option<String>,
    pub motion_start: Option<f64>,
    pub motion_end: Option<f64>,
    pub source_span: SourceSpan,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chart {
    pub duration_seconds: f64,
    pub notes: Vec<Note>,
    pub paths: Vec<SlidePath>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct SolverConfig {
    pub beam_width: usize,
    pub top_k: usize,
    pub allow_handover: bool,
    pub checkpoint_seconds: f64,
    pub contact_seconds: f64,
    pub handover_seconds: f64,
    pub handover_cooldown: f64,
    pub preparation_seconds: f64,
    pub speed_reference: f64,
    pub repetition_seconds: f64,
    pub distance_weight: f64,
    pub speed_weight: f64,
    pub side_weight: f64,
    pub cross_weight: f64,
    pub repetition_weight: f64,
    pub handover_weight: f64,
}
impl Default for SolverConfig {
    fn default() -> Self {
        Self {
            beam_width: 128,
            top_k: 3,
            allow_handover: true,
            checkpoint_seconds: 0.05,
            contact_seconds: 0.03,
            handover_seconds: 0.04,
            handover_cooldown: 0.2,
            preparation_seconds: 1.0,
            speed_reference: 4.0,
            repetition_seconds: 0.15,
            distance_weight: 1.0,
            speed_weight: 1.0,
            side_weight: 1.0,
            cross_weight: 1.0,
            repetition_weight: 1.0,
            handover_weight: 1.0,
        }
    }
}
impl SolverConfig {
    pub fn validate(&self) -> Result<(), String> {
        let values = [
            self.checkpoint_seconds,
            self.contact_seconds,
            self.handover_seconds,
            self.handover_cooldown,
            self.preparation_seconds,
            self.speed_reference,
            self.repetition_seconds,
        ];
        if values.iter().any(|v| !v.is_finite() || *v <= 0.0) {
            return Err("時間與速度參數必須為有限正數".into());
        }
        if !(1..=256).contains(&self.beam_width)
            || !(1..=5).contains(&self.top_k)
            || self.top_k > self.beam_width
        {
            return Err("beamWidth 必須為 1–256，topK 為 1–5 且不大於 beamWidth".into());
        }
        if !(0.02..=0.2).contains(&self.checkpoint_seconds)
            || self.handover_seconds > self.checkpoint_seconds
            || self.preparation_seconds > 10.0
        {
            return Err("checkpointSeconds 必須為 0.02–0.2；交接重疊不可超過 checkpoint；預備時間最多 10 秒".into());
        }
        if !(0.1..=100.0).contains(&self.speed_reference)
            || !(0.001..=1.0).contains(&self.contact_seconds)
            || !(0.001..=10.0).contains(&self.repetition_seconds)
            || !(0.01..=10.0).contains(&self.preparation_seconds)
            || !(0.001..=0.2).contains(&self.handover_seconds)
            || !(self.handover_seconds..=10.0).contains(&self.handover_cooldown)
        {
            return Err("速度參考須為 0.1–100、接觸時間 0.001–1 秒、連打間隔 0.001–10 秒、預備 0.01–10 秒、交接 0.001–0.2 秒，冷卻須介於交接時間與 10 秒".into());
        }
        if [
            self.distance_weight,
            self.speed_weight,
            self.side_weight,
            self.cross_weight,
            self.repetition_weight,
            self.handover_weight,
        ]
        .iter()
        .any(|v| !v.is_finite() || *v < 0.0 || *v > 100.0)
        {
            return Err("成本權重必須為 0–100 的有限數值".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeRequest {
    pub request_id: String,
    pub source: String,
    #[serde(default)]
    pub first_seconds: f64,
    #[serde(default)]
    pub solver_config: SolverConfig,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeResponse {
    pub schema_version: u32,
    pub request_id: String,
    pub status: String,
    pub diagnostics: Vec<Diagnostic>,
    pub chart: Option<Chart>,
    pub solutions: Vec<Solution>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Hand {
    L,
    R,
}
impl Hand {
    pub fn index(self) -> usize {
        if self == Self::L {
            0
        } else {
            1
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MotionSample {
    pub time_seconds: f64,
    pub x: f64,
    pub y: f64,
}
impl MotionSample {
    pub fn new(t: f64, p: Point) -> Self {
        Self {
            time_seconds: t,
            x: p.x,
            y: p.y,
        }
    }
    pub fn point(&self) -> Point {
        Point {
            x: self.x,
            y: self.y,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MotionSegment {
    pub mode: String,
    pub note_id: Option<String>,
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub samples: Vec<MotionSample>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Assignment {
    pub note_id: String,
    pub part: String,
    pub hand: Hand,
    pub start_seconds: f64,
    pub end_seconds: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Handover {
    pub note_id: String,
    pub from: Hand,
    pub to: Hand,
    pub start_seconds: f64,
    pub end_seconds: f64,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CostBreakdown {
    pub distance: f64,
    pub speed: f64,
    pub side: f64,
    pub cross: f64,
    pub repetition: f64,
    pub handover: f64,
}
impl CostBreakdown {
    pub fn total(&self) -> f64 {
        self.distance + self.speed + self.side + self.cross + self.repetition + self.handover
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Solution {
    pub id: String,
    pub total_cost: f64,
    pub cost_breakdown: CostBreakdown,
    pub assignments: Vec<Assignment>,
    pub handovers: Vec<Handover>,
    pub left_segments: Vec<MotionSegment>,
    pub right_segments: Vec<MotionSegment>,
    pub config_snapshot: SolverConfig,
    pub warnings: Vec<String>,
}
