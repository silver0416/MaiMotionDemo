//! V3 的「追蹤用譜面」：求解器照舊使用 motion_start／motion_end 與路徑取樣，
//! 這裡只換掉內部譜面的手部路線與結束時間；回傳給前端的譜面仍是原本的星星路徑。
//!
//! - 預設（畫完）：單條 Slide 沿星星路徑畫到終點，不改動。真人的偷懶是用手掌範圍
//!   同時解決多個音符，因此同時出發、同時結束的 Slide 若一隻手掌蓋得住兩顆星星，
//!   就多一條「掌心沿兩顆星星中點走完」的共用路線。
//! - `slideShortcut`：依判定佇列把路線換成抄近路線，手在尾判正解時刻完成最後一區。

use super::{slide_critical_window, EPS, JUDGE_FRAME};
use crate::judge::{self, Route};
use crate::{Chart, JudgeArea, PathSample, Point, SolverConfig};
use std::collections::{BTreeMap, BTreeSet};

/// 同時完成的判定區離正解區間邊緣至少留這麼多時間。
const WINDOW_MARGIN: f64 = 2.0 * JUDGE_FRAME;

fn path_index(chart: &Chart, note: usize) -> Option<usize> {
    let id = chart.notes[note].path_id.as_ref()?;
    chart.paths.iter().position(|p| &p.id == id)
}

/// 尾判正解時刻與 Critical Perfect 區間。
fn judge_window(chart: &Chart, note: usize) -> (f64, (f64, f64)) {
    let n = &chart.notes[note];
    let (start, end) = (n.motion_start.unwrap(), n.motion_end.unwrap());
    let progress = chart.paths[path_index(chart, note).unwrap()].judge_progress;
    (
        start + (end - start) * progress,
        slide_critical_window(start, end, progress),
    )
}

/// 把路線的弧長換成時間比例：手等速前進，但若有佇列提早完成（手掌同時覆蓋多條時），
/// 在那一點之前放慢，讓每條佇列的完成時間都落在自己的 Critical Perfect 區間內。
fn timed(
    route: &Route,
    start: f64,
    finish: f64,
    windows: &[(f64, f64)],
) -> Option<Vec<PathSample>> {
    let span = finish - start;
    if span <= EPS {
        return None;
    }
    let earliest = route.completions.iter().copied().fold(1.0, f64::min);
    // 最早完成的佇列需要的最早時間比例。
    let needed = route
        .completions
        .iter()
        .zip(windows)
        .filter(|(c, _)| (**c - earliest).abs() < 1e-9)
        .map(|(_, (low, _))| (low + WINDOW_MARGIN - start) / span)
        .fold(0.0, f64::max);
    let pivot = if earliest < 1.0 - 1e-9 && needed > earliest {
        Some((earliest, needed.min(1.0 - 1e-6)))
    } else {
        None
    };
    let tau = |a: f64| match pivot {
        Some((at, value)) if a <= at => a / at * value,
        Some((at, value)) => value + (a - at) / (1.0 - at) * (1.0 - value),
        None => a,
    };
    for (c, (low, high)) in route.completions.iter().zip(windows) {
        let t = start + tau(*c) * span;
        if t < low - EPS || t > high + EPS {
            return None;
        }
    }
    let arc = route.samples();
    let mut points: Vec<(f64, Point)> = arc
        .iter()
        .map(|s| (s.u, Point { x: s.x, y: s.y }))
        .collect();
    if let Some((at, _)) = pivot {
        if points.iter().all(|(u, _)| (u - at).abs() > 1e-9) {
            let position = crate::sample_at(&arc, at);
            let index = points.partition_point(|(u, _)| *u < at);
            points.insert(index, (at, position));
        }
    }
    Some(
        points
            .into_iter()
            .map(|(u, p)| PathSample {
                u: tau(u),
                x: p.x,
                y: p.y,
            })
            .collect(),
    )
}

/// 把 [f0, f1] 這段時間比例切出來，重新參數化成 0→1。
fn slice(samples: &[PathSample], f0: f64, f1: f64) -> Vec<PathSample> {
    let span = (f1 - f0).max(EPS);
    let mut out = vec![];
    let head = crate::sample_at(samples, f0);
    out.push(PathSample {
        u: 0.0,
        x: head.x,
        y: head.y,
    });
    for s in samples {
        if s.u > f0 + 1e-12 && s.u < f1 - 1e-12 {
            out.push(PathSample {
                u: (s.u - f0) / span,
                x: s.x,
                y: s.y,
            });
        }
    }
    let tail = crate::sample_at(samples, f1);
    out.push(PathSample {
        u: 1.0,
        x: tail.x,
        y: tail.y,
    });
    out
}

fn same(a: f64, b: f64) -> bool {
    (a - b).abs() < EPS
}

/// 同時出發、同時結束的兩條 Slide 由一隻手掌覆蓋、兩條都畫到終點：掌心沿兩顆星星的
/// 中點走，任何時刻兩顆星星都要在手掌半徑內。
fn palm_trace(a: &crate::SlidePath, b: &crate::SlidePath, palm: f64) -> Option<Vec<PathSample>> {
    const STEPS: usize = 128;
    let mut us: Vec<f64> = (0..=STEPS).map(|k| k as f64 / STEPS as f64).collect();
    us.extend(a.samples.iter().chain(&b.samples).map(|s| s.u));
    us.sort_by(f64::total_cmp);
    us.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
    let mut out = Vec::with_capacity(us.len());
    for u in us {
        let (p, q) = (a.at(u), b.at(u));
        if p.distance(q) * 0.5 > palm + EPS {
            return None;
        }
        let m = p.lerp(q, 0.5);
        out.push(PathSample { u, x: m.x, y: m.y });
    }
    Some(out)
}

pub(super) fn tracking_chart(chart: &Chart, c: &SolverConfig) -> Chart {
    let mut out = chart.clone();
    let slides: Vec<usize> = (0..chart.notes.len())
        .filter(|i| path_index(chart, *i).is_some())
        .collect();
    let path = |i: usize| &chart.paths[path_index(chart, i).unwrap()];
    let wifi = |i: usize| path(i).branches.len() == 2;

    // 接續寫法：沒有起點觸碰、從前一段的終點與結束時間接下去的本體。
    let mut child: BTreeMap<usize, usize> = BTreeMap::new();
    let mut has_parent = BTreeSet::new();
    for &j in &slides {
        let b = &chart.notes[j];
        if b.has_head || wifi(j) {
            continue;
        }
        let parent = slides.iter().copied().find(|&i| {
            let a = &chart.notes[i];
            i != j
                && !wifi(i)
                && !child.contains_key(&i)
                && a.source_span.start == b.source_span.start
                && a.source_span.end == b.source_span.end
                && same(a.motion_end.unwrap(), b.motion_start.unwrap())
                && path(i).end_button == path(j).start_button
        });
        if let Some(i) = parent {
            child.insert(i, j);
            has_parent.insert(j);
        }
    }

    let mut singles = vec![];
    for &first in &slides {
        if has_parent.contains(&first) || wifi(first) {
            continue;
        }
        let mut parts = vec![first];
        while let Some(&next) = child.get(parts.last().unwrap()) {
            parts.push(next);
        }
        if !c.slide_shortcut {
            if parts.len() == 1 {
                singles.push(first);
            }
            continue;
        }
        let queues: Vec<&[JudgeArea]> = parts
            .iter()
            .map(|i| path(*i).judge_areas.as_slice())
            .collect();
        if queues.iter().any(|q| q.is_empty()) {
            continue;
        }
        let joined = judge::join_queues(&queues);
        let start_button = path(first).start_button;
        let Some(route) = judge::route(path(first).at(0.0), &[&joined], None, start_button) else {
            continue;
        };
        let last = *parts.last().unwrap();
        let (finish, window) = judge_window(chart, last);
        let begin = chart.notes[first].motion_start.unwrap();
        let Some(samples) = timed(&route, begin, finish, &[window]) else {
            continue;
        };
        for (k, &note) in parts.iter().enumerate() {
            let n = &chart.notes[note];
            let t0 = n.motion_start.unwrap();
            let t1 = if k + 1 == parts.len() {
                finish
            } else {
                n.motion_end.unwrap()
            };
            let (f0, f1) = (
                (t0 - begin) / (finish - begin),
                (t1 - begin) / (finish - begin),
            );
            let p = path_index(chart, note).unwrap();
            out.paths[p].samples = slice(&samples, f0.clamp(0.0, 1.0), f1.clamp(0.0, 1.0));
            out.paths[p].judge_progress = 1.0;
            out.notes[note].motion_end = Some(t1);
            out.notes[note].end_seconds = t1;
        }
        if parts.len() == 1 {
            singles.push(first);
        }
    }

    let palm = (c.palm_radius > 0.0).then_some(c.palm_radius);

    // 同一時間出發、同一時間結束的 Slide：一隻手可張開同時覆蓋兩條。
    // 同組共用一個完成時刻（各自正解時刻的平均），且必須落在每一條的正解區間內。
    if let Some(palm) = palm {
        let mut groups: Vec<Vec<usize>> = vec![];
        for &i in &singles {
            let n = &chart.notes[i];
            let found = groups.iter_mut().find(|g| {
                let m = &chart.notes[g[0]];
                same(m.motion_start.unwrap(), n.motion_start.unwrap())
                    && same(m.motion_end.unwrap(), n.motion_end.unwrap())
            });
            match found {
                Some(group) => group.push(i),
                None => groups.push(vec![i]),
            }
        }
        for group in groups.into_iter().filter(|g| g.len() >= 2) {
            if !c.slide_shortcut {
                // 畫完：結束時間不變，掌心沿兩顆星星的中點走到終點。
                for (x, &i) in group.iter().enumerate() {
                    for &j in group.iter().skip(x + 1) {
                        let Some(samples) = palm_trace(path(i), path(j), palm) else {
                            continue;
                        };
                        let (pi, pj) =
                            (path_index(chart, i).unwrap(), path_index(chart, j).unwrap());
                        out.paths[pi].hand_routes.bundles.push((j, samples.clone()));
                        out.paths[pj].hand_routes.bundles.push((i, samples));
                    }
                }
                continue;
            }
            let windows: Vec<(f64, (f64, f64))> =
                group.iter().map(|i| judge_window(chart, *i)).collect();
            let finish = windows.iter().map(|(j, _)| j).sum::<f64>() / windows.len() as f64;
            if windows.iter().any(|(_, (low, high))| {
                finish < low + WINDOW_MARGIN || finish > high - WINDOW_MARGIN
            }) {
                continue;
            }
            for &i in &group {
                out.notes[i].motion_end = Some(finish);
                out.notes[i].end_seconds = finish;
            }
            let begin = chart.notes[group[0]].motion_start.unwrap();
            for (x, &i) in group.iter().enumerate() {
                for (y, &j) in group.iter().enumerate().skip(x + 1) {
                    let origin = path(i).at(0.0).lerp(path(j).at(0.0), 0.5);
                    let queues = [
                        path(i).judge_areas.as_slice(),
                        path(j).judge_areas.as_slice(),
                    ];
                    let Some(route) =
                        judge::route(origin, &queues, Some(palm), path(i).start_button)
                    else {
                        continue;
                    };
                    let Some(samples) = timed(&route, begin, finish, &[windows[x].1, windows[y].1])
                    else {
                        continue;
                    };
                    let (pi, pj) = (path_index(chart, i).unwrap(), path_index(chart, j).unwrap());
                    out.paths[pi].hand_routes.bundles.push((j, samples.clone()));
                    out.paths[pj].hand_routes.bundles.push((i, samples));
                }
            }
        }
    }

    // 畫完的 WiFi：三顆星星都走到終點。中央＋一側由同一手掌沿兩顆星星的中點覆蓋、
    // 另一側由另一手；或單手張開沿中央星星畫完，手掌範圍在正解區間內碰完三條佇列。
    if !c.slide_shortcut {
        for &i in slides.iter().filter(|i| wifi(**i)) {
            let p = path(i);
            if p.judge_areas.is_empty() || p.branch_judge_areas.len() != 2 {
                continue;
            }
            let mut routes = crate::HandRoutes::default();
            let side = |s: usize| crate::SlidePath {
                samples: p.branches[s].clone(),
                ..p.clone()
            };
            for s in 0..2 {
                routes.wifi_pair[s] = palm_trace(p, &side(s), c.palm_radius);
                routes.wifi_single[s] = Some(p.branches[s].clone());
            }
            if let Some(palm) = palm {
                // 單手：掌心沿兩條側線星星的中點掃過整個扇形直到終點（不抄近、不提早停），
                // 手掌範圍內的感應區都算碰到；三條佇列都要在正解區間內完成。
                let (_, window) = judge_window(chart, i);
                let n = &chart.notes[i];
                let (start, end) = (n.motion_start.unwrap(), n.motion_end.unwrap());
                let Some(sweep) = palm_trace(&side(0), &side(1), f64::INFINITY) else {
                    continue;
                };
                let points: Vec<Point> = sweep.iter().map(|s| Point { x: s.x, y: s.y }).collect();
                let total = judge::polyline_length(&points);
                let queues = [
                    p.judge_areas.as_slice(),
                    p.branch_judge_areas[0].as_slice(),
                    p.branch_judge_areas[1].as_slice(),
                ];
                let in_window =
                    judge::simulate_route(&points, &queues, Some(palm)).is_some_and(|done| {
                        total > EPS
                            && done.iter().all(|d| {
                                let t = start + d / total * (end - start);
                                t >= window.0 - EPS && t <= window.1 + EPS
                            })
                    });
                if in_window {
                    routes.wifi_all = Some(sweep);
                }
            }
            let usable = (0..2).any(|s| routes.wifi_pair[s].is_some()) || routes.wifi_all.is_some();
            if usable {
                out.paths[path_index(chart, i).unwrap()].hand_routes = routes;
            }
        }
        return out;
    }

    // WiFi：三條判定佇列。中央＋一側由同一手覆蓋、另一側由另一手，或單手張開覆蓋三條。
    for &i in slides.iter().filter(|i| wifi(**i)) {
        let p = path(i);
        if p.judge_areas.is_empty() || p.branch_judge_areas.len() != 2 {
            continue;
        }
        let (finish, window) = judge_window(chart, i);
        let begin = chart.notes[i].motion_start.unwrap();
        let origin = p.at(0.0);
        let center = p.judge_areas.as_slice();
        let sides = [
            p.branch_judge_areas[0].as_slice(),
            p.branch_judge_areas[1].as_slice(),
        ];
        let reach = Some(c.palm_radius);
        let solve = |queues: &[&[JudgeArea]], palm: Option<f64>| {
            judge::route(origin, queues, palm, p.start_button)
                .and_then(|r| timed(&r, begin, finish, &vec![window; queues.len()]))
        };
        let mut routes = crate::HandRoutes::default();
        for (s, side) in sides.iter().enumerate() {
            routes.wifi_pair[s] = solve(&[center, side], reach);
            routes.wifi_single[s] = solve(&[side], None);
        }
        if palm.is_some() {
            routes.wifi_all = solve(&[center, sides[0], sides[1]], reach);
        }
        let usable = (0..2)
            .any(|s| routes.wifi_pair[s].is_some() && routes.wifi_single[1 - s].is_some())
            || routes.wifi_all.is_some();
        if !usable {
            continue;
        }
        let pi = path_index(chart, i).unwrap();
        out.paths[pi].hand_routes = routes;
        out.paths[pi].judge_progress = 1.0;
        out.notes[i].motion_end = Some(finish);
        out.notes[i].end_seconds = finish;
    }
    out
}
