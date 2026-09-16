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

/// `p` `q` `pp` `qq` 的幾何，依 MajdataPlay SlideGeo 的圓與切線定義重建
/// （盤面半徑 1）。只用來產生可辨識的路徑，不是實機軌道座標。
///
/// - `p`／`q`：以盤心為圓心、半徑 cos(3π/8) 的圓；從起點切入，繞到可以切出往終點的角度。
/// - `pp`／`qq`：半徑 cos(π/8)/2、通過盤心的圓，圓心朝向起點旁第 3 個 D 區
///   （`pp` 為順時針方向的 D(起點+2)，`qq` 鏡射為 D(起點−1)）。弧長不足 90° 時多繞一圈，
///   對應 Majdata 判定表中 4、5 號相對終點會先繞完再穿過中心。
///
/// 旋轉方向以畫面上看為準：`p`／`pp` 逆時針，`q`／`qq` 順時針。
fn loop_points(start: u8, end: u8, ccw: bool, wide: bool) -> Vec<Point> {
    // 本檔座標 y 向下，畫面逆時針即角度遞減。
    let dir = if ccw { -1.0 } else { 1.0 };
    let (center, radius) = if wide {
        let b = (PI / 8.0).cos() / 2.0;
        let d = if ccw {
            wrap(start as i32 + 2)
        } else {
            wrap(start as i32 - 1)
        };
        // D 區 k 位於 k−1 與 k 號鍵之間。
        (polar(angle(d) - PI / 8.0, b), b)
    } else {
        (Point { x: 0.0, y: 0.0 }, (3.0 * PI / 8.0).cos())
    };
    let from = button(start);
    let to = button(end);
    // 從外部點沿 dir 方向切入圓的切點角度；切出則是反方向切線。
    let tangent = |p: Point, sign: f64| {
        let (dx, dy) = (p.x - center.x, p.y - center.y);
        let delta = (radius / dx.hypot(dy)).acos();
        dy.atan2(dx) + sign * delta
    };
    let enter = tangent(from, dir);
    let exit = tangent(to, -dir);
    let mut sweep = ((exit - enter) * dir).rem_euclid(TAU);
    if sweep < 0.002 || (wide && sweep < PI / 2.0) {
        sweep += TAU;
    }
    let mut points = vec![from];
    let n = (sweep * radius / 0.02).ceil().max(2.0) as usize;
    for i in 0..=n {
        let a = enter + dir * sweep * i as f64 / n as f64;
        points.push(Point {
            x: center.x + radius * a.cos(),
            y: center.y + radius * a.sin(),
        });
    }
    points.push(to);
    points
}

/// B 區判定節點（不是 B 區 Touch 的代表點）：Majdata 以 cos(3π/8)/cos(π/8) 為半徑。
fn b_node(k: u8) -> Point {
    polar(angle(k), (3.0 * PI / 8.0).cos() / (PI / 8.0).cos())
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
            // Majdata：轉折在逆時針側時，終點限順時針 1–4 格；轉折在順時針側時鏡射。
            let reach = if turn == wrap(start as i32 - 2) {
                d
            } else {
                (8 - d) % 8
            };
            if !(1..=4).contains(&reach) {
                return Err("V 形 Slide 的終點必須在轉折鍵另一側、距起點 1–4 格".into());
            }
            Ok(vec![button(start), button(turn), button(end)])
        }
        Shape::Loop { ccw, wide } => Ok(loop_points(start, end, ccw, wide)),
        Shape::S | Shape::Z => {
            if d != 4 {
                return Err("S 與 Z 形 Slide 只能連到正對面的鍵位".into());
            }
            // Majdata 的 `s` 判定佇列為 A1 B8 B7 C B3 B4 A5：先往起點逆時針側的內圈折入，
            // 穿過中心，再從對側內圈折出；`z` 為其鏡射。
            let side = if shape == Shape::S { -2 } else { 2 };
            Ok(vec![
                button(start),
                b_node(wrap(start as i32 + side)),
                b_node(wrap(end as i32 + side)),
                button(end),
            ])
        }
        Shape::Wifi => {
            if d != 4 {
                return Err("Wifi Slide 只能連到正對面的鍵位".into());
            }
            Ok(vec![button(start), button(end)])
        }
    }
}

/// 引導星星在最後一個判定區停留的時間佔這段 Slide 時長的比例，取自 MajdataPlay
/// SlideTables.cs 的 Const。Slide 尾判的正解時刻 = 移動開始 + 時長 × (1 − Const)，
/// 因此 Critical Perfect 區間會隨形狀與 Slide 長度改變。
fn judge_const(segment: &Segment) -> f64 {
    let Segment { start, end, shape } = *segment;
    // Majdata 以「相對終點」1–8 查表（1 為同鍵），逆向形狀用鏡射後的鍵查同一張表。
    let relative = ((end + 8 - start) % 8 + 1) as usize;
    let mirror = |r: usize| if r == 1 { 1 } else { 10 - r };
    let upper = matches!(start, 1 | 2 | 7 | 8);
    const LINE: [f64; 9] = [0.0, 0.0, 0.0, 0.182, 0.19, 0.152, 0.19, 0.182, 0.0];
    const CIRCLE: [f64; 9] = [0.0, 0.058, 0.465, 0.233, 0.155, 0.116, 0.093, 0.078, 0.066];
    const V: [f64; 9] = [0.0, 0.185, 0.15, 0.158, 0.158, 0.158, 0.158, 0.158, 0.154];
    const L: [f64; 9] = [0.0, 0.0, 0.1, 0.104, 0.098, 0.105, 0.0, 0.0, 0.0];
    const PPQQ: [f64; 9] = [0.0, 0.065, 0.086, 0.157, 0.065, 0.065, 0.067, 0.079, 0.0626];
    const PQ: [f64; 9] = [0.0, 0.095, 0.112, 0.125, 0.139, 0.160, 0.080, 0.084, 0.0895];
    let value = match shape {
        Shape::Line => LINE[relative],
        Shape::Arc('^') => {
            CIRCLE[if relative < 5 {
                relative
            } else {
                mirror(relative)
            }]
        }
        Shape::Arc(c) => {
            let clockwise = (c == '>') == upper;
            CIRCLE[if clockwise {
                relative
            } else {
                mirror(relative)
            }]
        }
        Shape::Center => V[relative],
        Shape::Grand(turn) => {
            if turn == wrap(start as i32 - 2) {
                L[relative]
            } else {
                L[mirror(relative)]
            }
        }
        Shape::Loop { ccw, wide } => {
            let r = if ccw { relative } else { mirror(relative) };
            if wide {
                PPQQ[r]
            } else {
                PQ[r]
            }
        }
        Shape::S | Shape::Z => 0.13,
        Shape::Wifi => 0.16287,
    };
    // 表外組合在前面的形狀檢查已被拒絕；保底用直線的典型值。
    if value > 0.0 {
        value
    } else {
        0.15
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
    let mut last_length = 0.0;
    for (i, segment) in segments.iter().enumerate() {
        let part = segment_points(segment)?;
        last_length = part.windows(2).map(|w| w[0].distance(w[1])).sum();
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
    let samples = resample(&points)?;
    let total: f64 = points.windows(2).map(|w| w[0].distance(w[1])).sum();
    // 連續寫法共用一段時間、依弧長等速前進，最後一段的停留比例換算到整條路徑上。
    let last = judge_const(segments.last().unwrap()) * last_length / total.max(1e-9);
    Ok(SlidePath {
        id,
        shape,
        start_button: segments[0].start,
        end_button: segments[segments.len() - 1].end,
        samples,
        branches,
        judge_progress: (1.0 - last).clamp(0.0, 1.0),
    })
}
