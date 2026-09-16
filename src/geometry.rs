use crate::{PathSample, Point, SlidePath, TouchSensor};
use std::f64::consts::{PI, TAU};

/// 第 k 鍵的角度，定義見 docs/RULEBOOK.md §3。
pub fn angle(k: u8) -> f64 {
    -PI / 2.0 + PI / 8.0 + (k as f64 - 1.0) * PI / 4.0
}

pub fn button(k: u8) -> Point {
    polar(angle(k), 1.0)
}

fn polar(theta: f64, r: f64) -> Point {
    Point {
        x: r * theta.cos(),
        y: r * theta.sin(),
    }
}

/// Touch 感應區半徑。A/D 在外圈、B/E 在內圈、C 在中心；
/// D 與 E 相對於 A 與 B 旋轉半格（22.5°）。這組半徑是 Demo 假設，不是實機尺寸。
pub const TOUCH_OUTER: f64 = 0.90;
pub const TOUCH_INNER: f64 = 0.42;

pub fn touch(area: char, index: u8) -> Point {
    match area {
        'A' => polar(angle(index), TOUCH_OUTER),
        'B' => polar(angle(index), TOUCH_INNER),
        'D' => polar(angle(index) - PI / 8.0, TOUCH_OUTER),
        'E' => polar(angle(index) - PI / 8.0, TOUCH_INNER),
        _ => Point { x: 0.0, y: 0.0 },
    }
}

pub fn touch_sensors() -> Vec<TouchSensor> {
    let mut sensors = Vec::with_capacity(33);
    for area in ['A', 'B', 'D', 'E'] {
        for index in 1..=8 {
            sensors.push(TouchSensor {
                area: area.to_string(),
                index,
                position: touch(area, index),
            });
        }
    }
    sensors.push(TouchSensor {
        area: "C".into(),
        index: 0,
        position: touch('C', 0),
    });
    sensors
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// `-` 直線
    Line,
    /// `^` `<` `>` 外圈圓弧
    Arc(char),
    /// `v` 經過中心的折線
    Center,
    /// `V` 大 V，附轉折鍵
    Grand(u8),
    /// `p` `q` `pp` `qq` 繞圈
    Loop { ccw: bool, wide: bool },
    /// `s` S 形
    S,
    /// `z` Z 形
    Z,
    /// `w` Wifi，主線為中央那條
    Wifi,
}

impl Shape {
    pub fn token(self) -> String {
        match self {
            Shape::Line => "-".into(),
            Shape::Arc(c) => c.to_string(),
            Shape::Center => "v".into(),
            Shape::Grand(_) => "V".into(),
            Shape::Loop { ccw, wide } => {
                let base = if ccw { 'p' } else { 'q' };
                if wide {
                    format!("{base}{base}")
                } else {
                    base.to_string()
                }
            }
            Shape::S => "s".into(),
            Shape::Z => "z".into(),
            Shape::Wifi => "w".into(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Segment {
    pub start: u8,
    pub end: u8,
    pub shape: Shape,
}

fn arc_steps(start: u8, end: u8, c: char) -> Result<f64, String> {
    let d = (end + 8 - start) % 8;
    match c {
        '^' => {
            if d == 0 || d == 4 {
                Err("^ 圓弧必須短於半圈，且起終點不同；對側或同鍵請改用 < 或 >".into())
            } else if d < 4 {
                Ok(d as f64)
            } else {
                Ok(d as f64 - 8.0)
            }
        }
        // simai 的 < 與 > 依起點落在上半或下半決定實際旋轉方向。
        _ => {
            let upper = matches!(start, 1 | 2 | 7 | 8);
            let clockwise = (c == '>') == upper;
            Ok(if clockwise {
                if d == 0 {
                    8.0
                } else {
                    d as f64
                }
            } else if d == 0 {
                -8.0
            } else {
                d as f64 - 8.0
            })
        }
    }
}

fn arc_points(start: u8, steps: f64) -> Vec<Point> {
    let from = angle(start);
    let sweep = steps * TAU / 8.0;
    let n = (sweep.abs() / 0.02).ceil().max(2.0) as usize;
    (0..=n)
        .map(|i| polar(from + sweep * i as f64 / n as f64, 1.0))
        .collect()
}

/// `p` `q` `pp` `qq`：從起點沿指定旋轉方向捲進內圈繞行，再捲出到終點。
/// 這是 Demo 的近似形狀，不是實機軌道座標；`pp` `qq` 用較大的繞行半徑。
fn loop_points(start: u8, end: u8, ccw: bool, wide: bool) -> Vec<Point> {
    let dir = if ccw { -1.0 } else { 1.0 };
    let rho = if wide { 0.72 } else { 0.42 };
    let from = angle(start);
    let to = angle(end);
    let enter = from + dir * PI / 4.0;
    let exit = to - dir * PI / 4.0;
    let mut sweep = ((exit - enter) * dir).rem_euclid(TAU);
    if sweep < 1e-9 {
        sweep = TAU;
    }
    let mut points = vec![];
    let ramp = 18;
    for i in 0..=ramp {
        let u = i as f64 / ramp as f64;
        let s = u * u * (3.0 - 2.0 * u);
        points.push(polar(from + (enter - from) * u, 1.0 + (rho - 1.0) * s));
    }
    let n = (sweep / 0.04).ceil().max(2.0) as usize;
    for i in 1..=n {
        points.push(polar(enter + dir * sweep * i as f64 / n as f64, rho));
    }
    for i in 1..=ramp {
        let u = i as f64 / ramp as f64;
        let s = u * u * (3.0 - 2.0 * u);
        points.push(polar(exit + (to - exit) * u, rho + (1.0 - rho) * s));
    }
    points
}

/// 以 Catmull-Rom 取樣通過控制點的平滑曲線，用於 S 與 Z。
fn smooth(control: &[Point], per: usize) -> Vec<Point> {
    let mut nodes = vec![control[0]];
    nodes.extend_from_slice(control);
    nodes.push(*control.last().unwrap());
    let mut points = vec![];
    for w in nodes.windows(4) {
        for i in 0..per {
            let t = i as f64 / per as f64;
            let t2 = t * t;
            let t3 = t2 * t;
            let x = 0.5
                * ((2.0 * w[1].x)
                    + (-w[0].x + w[2].x) * t
                    + (2.0 * w[0].x - 5.0 * w[1].x + 4.0 * w[2].x - w[3].x) * t2
                    + (-w[0].x + 3.0 * w[1].x - 3.0 * w[2].x + w[3].x) * t3);
            let y = 0.5
                * ((2.0 * w[1].y)
                    + (-w[0].y + w[2].y) * t
                    + (2.0 * w[0].y - 5.0 * w[1].y + 4.0 * w[2].y - w[3].y) * t2
                    + (-w[0].y + 3.0 * w[1].y - 3.0 * w[2].y + w[3].y) * t3);
            points.push(Point { x, y });
        }
    }
    points.push(*control.last().unwrap());
    points
}

fn wrap(k: i32) -> u8 {
    (((k - 1).rem_euclid(8)) + 1) as u8
}

pub fn segment_points(segment: &Segment) -> Result<Vec<Point>, String> {
    let Segment { start, end, shape } = *segment;
    let d = (end + 8 - start) % 8;
    match shape {
        Shape::Line => {
            if !(2..=6).contains(&d) {
                return Err("直線 Slide 終點必須與起點相隔 2–6 個鍵位".into());
            }
            Ok(vec![button(start), button(end)])
        }
        Shape::Arc(c) => Ok(arc_points(start, arc_steps(start, end, c)?)),
        Shape::Center => {
            if d == 0 {
                return Err("v 形 Slide 的起終點不可相同".into());
            }
            Ok(vec![button(start), Point { x: 0.0, y: 0.0 }, button(end)])
        }
        Shape::Grand(turn) => {
            if turn != wrap(start as i32 + 2) && turn != wrap(start as i32 - 2) {
                return Err("V 形 Slide 的轉折鍵必須是起點左右各兩格的鍵位".into());
            }
            if turn == end {
                return Err("V 形 Slide 的轉折鍵不可等於終點".into());
            }
            Ok(vec![button(start), button(turn), button(end)])
        }
        Shape::Loop { ccw, wide } => Ok(loop_points(start, end, ccw, wide)),
        Shape::S | Shape::Z => {
            if d != 4 {
                return Err("S 與 Z 形 Slide 只能連到正對面的鍵位".into());
            }
            let side = if shape == Shape::S { 2 } else { -2 };
            let bend = button(wrap(start as i32 + side));
            let first = Point {
                x: bend.x * 0.55,
                y: bend.y * 0.55,
            };
            let second = Point {
                x: -first.x,
                y: -first.y,
            };
            Ok(smooth(&[button(start), first, second, button(end)], 12))
        }
        Shape::Wifi => {
            if d != 4 {
                return Err("Wifi Slide 只能連到正對面的鍵位".into());
            }
            Ok(vec![button(start), button(end)])
        }
    }
}

/// Wifi 的兩條側線；主線仍是中央那條，手的移動以主線為準。
fn wifi_branches(segment: &Segment) -> Vec<Vec<PathSample>> {
    let mut out = vec![];
    for offset in [-1, 1] {
        let end = wrap(segment.end as i32 + offset);
        if let Ok(samples) = resample(&[button(segment.start), button(end)]) {
            out.push(samples);
        }
    }
    out
}

fn resample(points: &[Point]) -> Result<Vec<PathSample>, String> {
    let mut clean: Vec<Point> = vec![];
    for p in points {
        if clean.last().is_none_or(|q| q.distance(*p) > 1e-9) {
            clean.push(*p);
        }
    }
    if clean.len() < 2 {
        return Err("Slide 路徑長度為零".into());
    }
    let mut cumulative = vec![0.0];
    for w in clean.windows(2) {
        cumulative.push(cumulative.last().unwrap() + w[0].distance(w[1]));
    }
    let total = *cumulative.last().unwrap();
    Ok(clean
        .iter()
        .zip(cumulative)
        .map(|(p, c)| PathSample {
            u: c / total,
            x: p.x,
            y: p.y,
        })
        .collect())
}

/// 把一或多段形狀接成單一弧長參數化路徑。連續寫法（例如 `1-3-5[4:1]`）共用同一段時間。
pub fn build_path(id: String, segments: &[Segment]) -> Result<SlidePath, String> {
    if segments.is_empty() {
        return Err("Slide 缺少形狀".into());
    }
    let mut points: Vec<Point> = vec![];
    let mut shape = String::new();
    for (i, segment) in segments.iter().enumerate() {
        let part = segment_points(segment)?;
        shape.push_str(&segment.shape.token());
        if i == 0 {
            points.extend(part);
        } else {
            points.extend(part.into_iter().skip(1));
        }
    }
    let branches = if segments.len() == 1 && segments[0].shape == Shape::Wifi {
        wifi_branches(&segments[0])
    } else {
        vec![]
    };
    Ok(SlidePath {
        id,
        shape,
        start_button: segments[0].start,
        end_button: segments[segments.len() - 1].end,
        samples: resample(&points)?,
        branches,
    })
}
