//! Slide 判定佇列與依判定區抄近的手部路線。
//!
//! 判定規則取自 MajdataPlay（SlideBase.SensorCheck、SlideArea、SlideTables.cs），
//! 是社群重製版的規則，不是官方規格：
//! - Slide 是一串判定區，每個判定區是一或多個感應區，碰到任一個就算碰到。
//! - 每幀只看「目前」與「下一個」判定區。碰到下一個時，目前這個若可跳過或已碰過就直接移除，
//!   同一幀可以連續推進。非最後的判定區碰到後離開才算完成；最後一區碰到即完成。
//! - 因此可以跳區，但不能連續跳兩區；標記為不可跳過的判定區一定要碰到。
//! - 判定只看碰觸順序與最後一區的完成時間，不要求手跟著星星的路徑走。
//!
//! 感應區以代表點加上近似半徑的圓表示，手以指尖（或張開的手掌）圓近似，這是 Demo 幾何。

use crate::geometry::{touch, Segment, Shape};
use crate::{JudgeArea, PathSample, Point};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use std::sync::{Mutex, OnceLock};

/// 指尖接觸半徑（盤面半徑為 1）。Demo 假設。
pub const FINGER_RADIUS: f64 = 0.06;

/// 感應區代表點到區域邊緣的近似距離，依 resource/maimai.png 的感應區配置量測，取偏小值：
/// A 外圈大區、B 內圈五邊形、C 中央圓、D 外緣窄區、E 鍵位之間的菱形。
fn sensor_size(area: char) -> f64 {
    match area {
        'A' => 0.15,
        'B' => 0.14,
        'C' => 0.25,
        'D' => 0.07,
        _ => 0.10,
    }
}

/// 指尖碰到該感應區時，指尖中心與代表點的最大距離。
pub fn finger_reach(area: char) -> f64 {
    sensor_size(area) + FINGER_RADIUS
}

fn parse_sensor(name: &str) -> (char, u8) {
    let mut chars = name.chars();
    let area = chars.next().unwrap_or('C');
    (area, chars.as_str().parse().unwrap_or(0))
}

pub fn sensor_point(name: &str) -> Point {
    let (area, index) = parse_sensor(name);
    touch(area, index)
}

fn wrap(k: i32) -> u8 {
    (((k - 1).rem_euclid(8)) + 1) as u8
}

// 判定佇列（起點為 1 鍵）。以空白分隔判定區，`|` 分隔同一判定區的多個感應區，
// `!` 表示不可跳過。索引為相對終點 1–8（1 為同鍵），空字串為不存在的組合。
const LINE: [&str; 9] = [
    "",
    "",
    "",
    "A1 A2|B2! A3",
    "A1 B2 B3 A4",
    "A1 B1 C B5 A5",
    "A1 B8 B7 A6",
    "A1 A8|B8! A7",
    "",
];
const CIRCLE: [&str; 9] = [
    "",
    "A1 A2 A3 A4 A5 A6 A7 A8 A1",
    "A1! A2",
    "A1 A2! A3",
    "A1 A2 A3 A4",
    "A1 A2 A3 A4 A5",
    "A1 A2 A3 A4 A5 A6",
    "A1 A2 A3 A4 A5 A6 A7",
    "A1 A2 A3 A4 A5 A6 A7 A8",
];
const V: [&str; 9] = [
    "",
    "A1 B1 C B1 A1",
    "A1 B1 C B2 A2",
    "A1 B1 C B3 A3",
    "A1 B1 C B4 A4",
    "",
    "A1 B1 C B6 A6",
    "A1 B1 C B7 A7",
    "A1 B1 C B8 A8",
];
/// 大 V（轉折在起點逆時針側兩格）。
const L: [&str; 9] = [
    "",
    "",
    "A1 B8|A8! A7 B8 B1 A2",
    "A1 B8|A8! A7 B7 C B3 A3",
    "A1 B8|A8! A7 B6 B5 A4",
    "A1 B8|A8! A7 B6|A6! A5",
    "",
    "",
    "",
];
const PPQQ: [&str; 9] = [
    "",
    "A1 B1 C B4 A3 A2 A1",
    "A1 B1 C B4 A3 A2",
    "A1 B1 C B4 A3",
    "A1 B1 C B4 A3 A2 B1 C B4 A4",
    "A1 B1 C B4 A3 A2 B1 C B5 A5",
    "A1 B1 C B4 A3 A2 B1 C|B8 B7|B6 A6",
    "A1 B1 C B4 A3 A2 B1 B8 A7",
    "A1 B1 C B4 A3 A2 B1|A1 A8",
];
const PQ: [&str; 9] = [
    "",
    "A1 B8 B7 B6 B5 B4 B3 B2 A1",
    "A1 B8 B7 B6 B5 B4 B3 A2",
    "A1 B8 B7 B6 B5 B4 A3",
    "A1 B8 B7 B6 B5 A4",
    "A1 B8 B7 B6 A5",
    "A1 B8 B7 B6 B5 B4 B3 B2 B1 B8 B7 A6",
    "A1 B8 B7 B6 B5 B4 B3 B2 B1 B8 A7",
    "A1 B8 B7 B6 B5 B4 B3 B2 B1 A8",
];
const S: &str = "A1 B8 B7 C B3 B4 A5";
const WIFI_CENTER: &str = "A1 B1 C A5|B5";
/// 終點逆時針側那條（branches[0]，終點 −1 鍵）。
const WIFI_RIGHT: &str = "A1 B2 B3 A4|D5";
/// 終點順時針側那條（branches[1]，終點 +1 鍵）。
const WIFI_LEFT: &str = "A1 B8 B7 A6|D6";

/// 以 1 鍵為軸鏡射，再旋轉到起點。
fn transform(name: &str, mirror: bool, start: u8) -> String {
    let (area, index) = parse_sensor(name);
    if area == 'C' {
        return "C".into();
    }
    let mut k = index as i32;
    if mirror {
        // A／B 在鍵位方向；D／E 在兩鍵之間（偏逆時針半格），鏡射軸相差一格。
        k = if matches!(area, 'D' | 'E') {
            3 - k
        } else {
            2 - k
        };
    }
    format!("{area}{}", wrap(k + start as i32 - 1))
}

fn parse_table(table: &str, mirror: bool, start: u8) -> Vec<JudgeArea> {
    table
        .split_whitespace()
        .map(|token| {
            let skippable = !token.ends_with('!');
            JudgeArea {
                sensors: token
                    .trim_end_matches('!')
                    .split('|')
                    .map(|name| transform(name, mirror, start))
                    .collect(),
                skippable,
            }
        })
        .collect()
}

/// 單一形狀段的判定佇列。表外組合回傳空佇列（形狀檢查已先拒絕）。
pub fn segment_queue(segment: &Segment) -> Vec<JudgeArea> {
    let Segment { start, end, shape } = *segment;
    let r = ((end + 8 - start) % 8 + 1) as usize;
    let mirror_key = |r: usize| if r == 1 { 1 } else { 10 - r };
    let upper = matches!(start, 1 | 2 | 7 | 8);
    let (table, mirror) = match shape {
        Shape::Line => (LINE[r], false),
        Shape::Arc('^') => {
            if r < 5 {
                (CIRCLE[r], false)
            } else {
                (CIRCLE[mirror_key(r)], true)
            }
        }
        Shape::Arc(c) => {
            if (c == '>') == upper {
                (CIRCLE[r], false)
            } else {
                (CIRCLE[mirror_key(r)], true)
            }
        }
        Shape::Center => (V[r], false),
        Shape::Grand(turn) => {
            if turn == wrap(start as i32 - 2) {
                (L[r], false)
            } else {
                (L[mirror_key(r)], true)
            }
        }
        Shape::Loop { ccw, wide } => {
            let table = if wide { &PPQQ } else { &PQ };
            if ccw {
                (table[r], false)
            } else {
                (table[mirror_key(r)], true)
            }
        }
        Shape::S => (S, false),
        Shape::Z => (S, true),
        Shape::Wifi => (WIFI_CENTER, false),
    };
    parse_table(table, mirror, start)
}

/// 連續寫法共用一段時間時，各段的判定佇列接起來；接點是同一個 A 區，只保留一次。
pub fn path_queue(segments: &[Segment]) -> Vec<JudgeArea> {
    let mut out: Vec<JudgeArea> = vec![];
    for (i, segment) in segments.iter().enumerate() {
        let queue = segment_queue(segment);
        if queue.is_empty() {
            return vec![];
        }
        out.extend(queue.into_iter().skip(usize::from(i > 0)));
    }
    out
}

/// WiFi 兩條側線的判定佇列，順序與 SlidePath.branches 相同。
pub fn wifi_branch_queues(start: u8) -> Vec<Vec<JudgeArea>> {
    vec![
        parse_table(WIFI_RIGHT, false, start),
        parse_table(WIFI_LEFT, false, start),
    ]
}

/// 接起接續寫法的前後兩段佇列：接點的 A 區只保留一次。
pub fn join_queues(parts: &[&[JudgeArea]]) -> Vec<JudgeArea> {
    let mut out: Vec<JudgeArea> = vec![];
    for (i, part) in parts.iter().enumerate() {
        let skip = usize::from(
            i > 0 && out.last().map(|a| &a.sensors) == part.first().map(|a| &a.sensors),
        );
        out.extend(part.iter().skip(skip).cloned());
    }
    out
}

/// 一條手部路線：折線頂點，以及每條佇列在路線上完成時的弧長比例（0–1）。
#[derive(Clone, Debug)]
pub struct Route {
    pub points: Vec<Point>,
    pub completions: Vec<f64>,
}

impl Route {
    /// 弧長參數化的取樣點，u 0→1。長度為零時仍回傳兩點，位置不動。
    pub fn samples(&self) -> Vec<PathSample> {
        to_samples(&self.points)
    }
}

pub fn polyline_length(points: &[Point]) -> f64 {
    points.windows(2).map(|w| w[0].distance(w[1])).sum()
}

pub fn to_samples(points: &[Point]) -> Vec<PathSample> {
    let total = polyline_length(points);
    let mut out = vec![];
    let mut walked = 0.0;
    for (i, p) in points.iter().enumerate() {
        if i > 0 {
            walked += points[i - 1].distance(*p);
        }
        out.push(PathSample {
            u: if total > 1e-12 {
                walked / total
            } else {
                i as f64 / (points.len() - 1).max(1) as f64
            },
            x: p.x,
            y: p.y,
        });
    }
    if out.len() == 1 {
        let mut last = out[0].clone();
        last.u = 1.0;
        out.push(last);
    }
    out
}

struct Sensor {
    center: Point,
    reach: f64,
}

/// 求解用的佇列：[queue][area] -> 感應區圓。
struct Prepared {
    areas: Vec<Vec<Vec<Sensor>>>,
    skippable: Vec<Vec<bool>>,
}

impl Prepared {
    fn new(queues: &[&[JudgeArea]], palm: Option<f64>) -> Self {
        let areas = queues
            .iter()
            .map(|queue| {
                queue
                    .iter()
                    .map(|area| {
                        area.sensors
                            .iter()
                            .map(|name| {
                                let finger = finger_reach(parse_sensor(name).0);
                                Sensor {
                                    center: sensor_point(name),
                                    reach: palm.map_or(finger, |r| r.max(finger)),
                                }
                            })
                            .collect()
                    })
                    .collect()
            })
            .collect();
        let skippable = queues
            .iter()
            .map(|queue| queue.iter().map(|a| a.skippable).collect())
            .collect();
        Self { areas, skippable }
    }

    fn hits(&self, p: Point, q: usize, a: usize) -> bool {
        self.areas[q][a]
            .iter()
            .any(|s| p.distance(s.center) <= s.reach + 1e-9)
    }

    /// 手停在 p 時，佇列 q 從「最後碰到 s」推進到哪裡（-1 表示還沒碰到任何判定區）。
    /// 碰到下一區就前進；目前這區已離開、下一區可跳過時，碰到下下區也前進。
    fn advance(&self, p: Point, q: usize, mut s: i32) -> i32 {
        let n = self.areas[q].len() as i32;
        loop {
            let next = s + 1;
            if next >= n {
                return s;
            }
            if self.hits(p, q, next as usize) {
                s = next;
                continue;
            }
            let after = s + 2;
            if after < n
                && self.skippable[q][next as usize]
                && (s < 0 || !self.hits(p, q, s as usize))
                && self.hits(p, q, after as usize)
            {
                s = after;
                continue;
            }
            return s;
        }
    }

    fn done(&self, states: &[i32]) -> bool {
        states
            .iter()
            .enumerate()
            .all(|(q, s)| *s == self.areas[q].len() as i32 - 1)
    }

    /// 沿折線逐步前進，回傳每條佇列完成時的弧長；有佇列沒完成則為 None。
    fn simulate(&self, points: &[Point]) -> Option<Vec<f64>> {
        const STEP: f64 = 0.004;
        let mut states = vec![-1; self.areas.len()];
        let mut done: Vec<Option<f64>> = vec![None; self.areas.len()];
        let mut walked = 0.0;
        let mut visit = |p: Point, walked: f64, states: &mut Vec<i32>| {
            for q in 0..states.len() {
                if done[q].is_none() {
                    states[q] = self.advance(p, q, states[q]);
                    if states[q] == self.areas[q].len() as i32 - 1 {
                        done[q] = Some(walked);
                    }
                }
            }
        };
        for pair in points.windows(2) {
            let d = pair[0].distance(pair[1]);
            let steps = (d / STEP).ceil().max(1.0) as usize;
            for k in 1..=steps {
                let t = k as f64 / steps as f64;
                visit(pair[0].lerp(pair[1], t), walked + d * t, &mut states);
            }
            walked += d;
        }
        if points.len() == 1 {
            visit(points[0], 0.0, &mut states);
        }
        done.into_iter().collect()
    }
}

#[derive(PartialEq)]
struct Open(f64, usize);
impl Eq for Open {}
impl Ord for Open {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .0
            .total_cmp(&self.0)
            .then_with(|| other.1.cmp(&self.1))
    }
}
impl PartialOrd for Open {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// 候選接觸點與它來自哪個感應區圓（精修時沿著這個圓移動）；交點候選沒有單一來源。
struct Candidate {
    point: Point,
    owner: Option<(usize, usize, usize)>,
}

fn candidates(prepared: &Prepared, palm: bool) -> Vec<Candidate> {
    const RING: usize = 24;
    let mut out = vec![];
    let mut circles = vec![];
    for (q, queue) in prepared.areas.iter().enumerate() {
        for (a, area) in queue.iter().enumerate() {
            for (k, sensor) in area.iter().enumerate() {
                circles.push((q, sensor.center, sensor.reach));
                out.push(Candidate {
                    point: sensor.center,
                    owner: Some((q, a, k)),
                });
                let r = sensor.reach * (1.0 - 1e-6);
                for i in 0..RING {
                    let theta = i as f64 * std::f64::consts::TAU / RING as f64;
                    out.push(Candidate {
                        point: Point {
                            x: sensor.center.x + r * theta.cos(),
                            y: sensor.center.y + r * theta.sin(),
                        },
                        owner: Some((q, a, k)),
                    });
                }
            }
        }
    }
    if palm {
        // 兩個圓的交點附近能同時碰到兩條佇列的判定區。
        for (i, &(qa, ca, ra)) in circles.iter().enumerate() {
            for &(qb, cb, rb) in &circles[i + 1..] {
                if qa == qb {
                    continue;
                }
                let d = ca.distance(cb);
                if d < 1e-9 || d > ra + rb || d < (ra - rb).abs() {
                    continue;
                }
                let along = (ra * ra - rb * rb + d * d) / (2.0 * d);
                let h = (ra * ra - along * along).max(0.0).sqrt();
                let ux = (cb.x - ca.x) / d;
                let uy = (cb.y - ca.y) / d;
                let base = Point {
                    x: ca.x + ux * along,
                    y: ca.y + uy * along,
                };
                let middle = ca.lerp(cb, 0.5);
                for sign in [-1.0, 1.0] {
                    let p = Point {
                        x: base.x - sign * uy * h,
                        y: base.y + sign * ux * h,
                    };
                    out.push(Candidate {
                        point: p.lerp(middle, 1e-5),
                        owner: None,
                    });
                }
            }
        }
    }
    out
}

/// 依判定規則模擬一隻手沿折線移動（第一點視為起點、尚未碰到螢幕），回傳每條佇列
/// 完成時走過的弧長；有佇列沒完成則為 None。palm 為 None 時以指尖接觸。
pub fn simulate_route(
    points: &[Point],
    queues: &[&[JudgeArea]],
    palm: Option<f64>,
) -> Option<Vec<f64>> {
    Prepared::new(queues, palm).simulate(points)
}

/// 在所有佇列的判定規則下，從 origin 出發的最短折線（Dijkstra）。
/// 多條佇列（手掌同時覆蓋）時，完成必須同時發生在最後一點。
fn shortest(prepared: &Prepared, origin: Point, cands: &[Candidate]) -> Option<Vec<usize>> {
    let multi = prepared.areas.len() > 1;
    // hit[c][q][a]
    let hit: Vec<Vec<Vec<bool>>> = cands
        .iter()
        .map(|c| {
            (0..prepared.areas.len())
                .map(|q| {
                    (0..prepared.areas[q].len())
                        .map(|a| prepared.hits(c.point, q, a))
                        .collect()
                })
                .collect()
        })
        .collect();
    // by_area[q][a] = 碰得到該判定區的候選。
    let by_area: Vec<Vec<Vec<usize>>> = (0..prepared.areas.len())
        .map(|q| {
            (0..prepared.areas[q].len())
                .map(|a| (0..cands.len()).filter(|c| hit[*c][q][a]).collect())
                .collect()
        })
        .collect();

    // 節點：(各佇列狀態, 目前所在的候選；usize::MAX 為起點)。
    let mut index: HashMap<(Vec<i32>, usize), usize> = HashMap::new();
    let mut nodes: Vec<(Vec<i32>, usize, f64, usize)> = vec![]; // states, cand, dist, parent
    let start = vec![-1; prepared.areas.len()];
    index.insert((start.clone(), usize::MAX), 0);
    nodes.push((start, usize::MAX, 0.0, usize::MAX));
    let mut heap = BinaryHeap::new();
    heap.push(Open(0.0, 0));
    let mut settled = vec![false];
    while let Some(Open(dist, id)) = heap.pop() {
        if settled[id] {
            continue;
        }
        settled[id] = true;
        let (states, at, _, _) = nodes[id].clone();
        if prepared.done(&states) {
            let mut path = vec![];
            let mut cursor = id;
            while nodes[cursor].1 != usize::MAX {
                path.push(nodes[cursor].1);
                cursor = nodes[cursor].3;
            }
            path.reverse();
            return Some(path);
        }
        let here = if at == usize::MAX {
            origin
        } else {
            cands[at].point
        };
        let mut targets: Vec<usize> = vec![];
        for (q, &s) in states.iter().enumerate() {
            let n = prepared.areas[q].len() as i32;
            for next in [s + 1, s + 2] {
                if next >= n || (next == s + 2 && !prepared.skippable[q][(s + 1) as usize]) {
                    continue;
                }
                targets.extend(&by_area[q][next as usize]);
            }
        }
        targets.sort_unstable();
        targets.dedup();
        for c in targets {
            let point = cands[c].point;
            let mut changed = false;
            let mut newly_done = false;
            let mut next_states = states.clone();
            for q in 0..next_states.len() {
                let n = prepared.areas[q].len() as i32;
                let s = advance_by(&hit[c][q], &prepared.skippable[q], states[q]);
                if s != states[q] {
                    changed = true;
                    if s == n - 1 {
                        newly_done = true;
                    }
                }
                next_states[q] = s;
            }
            if !changed || (multi && newly_done && !prepared.done(&next_states)) {
                continue;
            }
            let cost = dist + here.distance(point);
            let key = (next_states.clone(), c);
            match index.get(&key) {
                Some(&existing) if nodes[existing].2 <= cost + 1e-12 => {}
                Some(&existing) => {
                    nodes[existing].2 = cost;
                    nodes[existing].3 = id;
                    heap.push(Open(cost, existing));
                }
                None => {
                    let new_id = nodes.len();
                    nodes.push((next_states, c, cost, id));
                    settled.push(false);
                    index.insert(key, new_id);
                    heap.push(Open(cost, new_id));
                }
            }
        }
    }
    None
}

/// 與 Prepared::advance 相同，但用預先算好的碰觸表。
fn advance_by(hit: &[bool], skippable: &[bool], mut s: i32) -> i32 {
    let n = hit.len() as i32;
    loop {
        let next = s + 1;
        if next >= n {
            return s;
        }
        if hit[next as usize] {
            s = next;
            continue;
        }
        let after = s + 2;
        if after < n
            && skippable[next as usize]
            && (s < 0 || !hit[s as usize])
            && hit[after as usize]
        {
            s = after;
            continue;
        }
        return s;
    }
}

/// 拉直：每個接觸點在自己的感應區圓內移到讓前後兩段最短的位置，驗證仍然合法才採用。
fn refine(prepared: &Prepared, origin: Point, cands: &[Candidate], path: &[usize]) -> Vec<Point> {
    let mut points: Vec<Point> = std::iter::once(origin)
        .chain(path.iter().map(|c| cands[*c].point))
        .collect();
    let owners: Vec<Option<(usize, usize, usize)>> = std::iter::once(None)
        .chain(path.iter().map(|c| cands[*c].owner))
        .collect();
    for _ in 0..4 {
        let mut improved = false;
        for i in 1..points.len() {
            let Some((q, a, k)) = owners[i] else {
                continue;
            };
            let sensor = &prepared.areas[q][a][k];
            let (center, radius) = (sensor.center, sensor.reach * (1.0 - 1e-6));
            let prev = points[i - 1];
            let next = points.get(i + 1).copied();
            let cost = |p: Point| prev.distance(p) + next.map_or(0.0, |n| p.distance(n));
            let proposal = best_in_disk(center, radius, prev, next);
            if cost(proposal) + 1e-9 >= cost(points[i]) {
                continue;
            }
            let mut trial = points.clone();
            trial[i] = proposal;
            if prepared.simulate(&trial).is_some() {
                points = trial;
                improved = true;
            }
        }
        if !improved {
            break;
        }
    }
    points
}

/// 圓盤內讓 |a−p|+|p−b| 最小的點；沒有 b 時為最靠近 a 的點。
fn best_in_disk(center: Point, radius: f64, a: Point, b: Option<Point>) -> Point {
    let clamp = |p: Point| {
        let d = p.distance(center);
        if d <= radius {
            p
        } else {
            center.lerp(p, radius / d)
        }
    };
    let Some(b) = b else {
        return clamp(a);
    };
    // 線段 ab 穿過圓盤：取線段上最靠近圓心的點，路線等於直接走 ab。
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let length = dx * dx + dy * dy;
    let t = if length > 1e-18 {
        (((center.x - a.x) * dx + (center.y - a.y) * dy) / length).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let closest = Point {
        x: a.x + dx * t,
        y: a.y + dy * t,
    };
    if closest.distance(center) <= radius {
        return closest;
    }
    let mut best = clamp(closest);
    let mut best_cost = a.distance(best) + best.distance(b);
    for i in 0..96 {
        let theta = i as f64 * std::f64::consts::TAU / 96.0;
        let p = Point {
            x: center.x + radius * theta.cos(),
            y: center.y + radius * theta.sin(),
        };
        let cost = a.distance(p) + p.distance(b);
        if cost < best_cost - 1e-12 {
            best = p;
            best_cost = cost;
        }
    }
    best
}

fn solve_route(origin: Point, queues: &[&[JudgeArea]], palm: Option<f64>) -> Option<Route> {
    if queues.is_empty() || queues.iter().any(|q| q.is_empty()) {
        return None;
    }
    let prepared = Prepared::new(queues, palm);
    let cands = candidates(&prepared, queues.len() > 1);
    let path = shortest(&prepared, origin, &cands)?;
    let mut points = refine(&prepared, origin, &cands, &path);
    let completions = prepared.simulate(&points)?;
    // 在最後一條佇列完成的位置截斷：之後不必再移動。
    let finish = completions.iter().copied().fold(0.0, f64::max);
    let mut walked = 0.0;
    let mut cut = vec![points[0]];
    for pair in points.windows(2) {
        let d = pair[0].distance(pair[1]);
        if walked + d >= finish - 1e-12 {
            let t = if d > 1e-12 {
                ((finish - walked) / d).clamp(0.0, 1.0)
            } else {
                1.0
            };
            cut.push(pair[0].lerp(pair[1], t));
            break;
        }
        cut.push(pair[1]);
        walked += d;
    }
    points = cut;
    let total = polyline_length(&points);
    Some(Route {
        completions: completions
            .iter()
            .map(|c| {
                if total > 1e-12 {
                    (c / total).clamp(0.0, 1.0)
                } else {
                    1.0
                }
            })
            .collect(),
        points,
    })
}

/// 旋轉對稱：先把起點轉到 1 鍵方向求解（可快取），再轉回來。
fn rotate(p: Point, turns: i32) -> Point {
    let theta = turns as f64 * std::f64::consts::FRAC_PI_4;
    Point {
        x: p.x * theta.cos() - p.y * theta.sin(),
        y: p.x * theta.sin() + p.y * theta.cos(),
    }
}

fn rotate_queue(queue: &[JudgeArea], turns: i32) -> Vec<JudgeArea> {
    queue
        .iter()
        .map(|area| JudgeArea {
            sensors: area
                .sensors
                .iter()
                .map(|name| {
                    let (a, k) = parse_sensor(name);
                    if a == 'C' {
                        "C".into()
                    } else {
                        format!("{a}{}", wrap(k as i32 + turns))
                    }
                })
                .collect(),
            skippable: area.skippable,
        })
        .collect()
}

type RouteKey = String;
fn cache() -> &'static Mutex<HashMap<RouteKey, Option<Route>>> {
    static CACHE: OnceLock<Mutex<HashMap<RouteKey, Option<Route>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 依判定規則從 origin 出發、完成所有佇列的最短手部路線。
/// palm 為 None 時以指尖接觸；Some(r) 表示張開的手掌，掌心 r 內的感應區都碰得到。
/// 求解與快取在「起點轉到 1 鍵」的座標系進行，結果與旋轉無關而且確定。
pub fn route(
    origin: Point,
    queues: &[&[JudgeArea]],
    palm: Option<f64>,
    start: u8,
) -> Option<Route> {
    let turns = -(start as i32 - 1);
    let local_origin = rotate(origin, turns);
    let local: Vec<Vec<JudgeArea>> = queues.iter().map(|q| rotate_queue(q, turns)).collect();
    let key = format!(
        "{:.9},{:.9}|{:?}|{}",
        local_origin.x,
        local_origin.y,
        palm.map(|r| (r * 1e9).round() as i64),
        local
            .iter()
            .map(|q| q
                .iter()
                .map(|a| format!(
                    "{}{}",
                    a.sensors.join("|"),
                    if a.skippable { "" } else { "!" }
                ))
                .collect::<Vec<_>>()
                .join(" "))
            .collect::<Vec<_>>()
            .join(" / ")
    );
    let solved = {
        let cached = cache().lock().unwrap().get(&key).cloned();
        match cached {
            Some(hit) => hit,
            None => {
                let refs: Vec<&[JudgeArea]> = local.iter().map(|q| q.as_slice()).collect();
                let result = solve_route(local_origin, &refs, palm);
                cache().lock().unwrap().insert(key, result.clone());
                result
            }
        }
    }?;
    Some(Route {
        points: solved.points.iter().map(|p| rotate(*p, -turns)).collect(),
        completions: solved.completions,
    })
}
