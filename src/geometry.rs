use crate::{PathSample, Point, SlidePath};
use std::f64::consts::{PI, TAU};

pub fn button(k: u8) -> Point {
    let a = -PI / 2.0 + PI / 8.0 + (k as f64 - 1.0) * PI / 4.0;
    Point {
        x: a.cos(),
        y: a.sin(),
    }
}

pub fn path(id: String, start: u8, end: u8, shape: char) -> Result<SlidePath, String> {
    let d = (end + 8 - start) % 8;
    let a = button(start);
    let b = button(end);
    if shape == '-' {
        if !(2..=6).contains(&d) {
            return Err("直線 Slide 終點必須與起點相隔 2–6 個鍵位".into());
        }
        return Ok(SlidePath {
            id,
            samples: vec![
                PathSample {
                    u: 0.0,
                    x: a.x,
                    y: a.y,
                },
                PathSample {
                    u: 1.0,
                    x: b.x,
                    y: b.y,
                },
            ],
        });
    }
    let steps = match shape {
        '^' if d > 0 && d != 4 => {
            if d < 4 {
                d as f64
            } else {
                d as f64 - 8.0
            }
        }
        '^' => return Err("^ 圓弧必須短於半圈，且起終點不同".into()),
        // simai < and > describe the direction at the upper/lower starting side.
        '>' | '<' => {
            let upper = matches!(start, 1 | 2 | 7 | 8);
            let clockwise = (shape == '>') == upper;
            if clockwise {
                if d == 0 {
                    8.0
                } else {
                    d as f64
                }
            } else if d == 0 {
                -8.0
            } else {
                d as f64 - 8.0
            }
        }
        _ => return Err("尚未支援此 Slide 形狀".into()),
    };
    let start_angle = a.y.atan2(a.x);
    let angle = steps * TAU / 8.0;
    let n = (angle.abs() / 0.02).ceil() as usize;
    let samples = (0..=n)
        .map(|i| {
            let u = i as f64 / n as f64;
            let theta = start_angle + u * angle;
            PathSample {
                u,
                x: theta.cos(),
                y: theta.sin(),
            }
        })
        .collect();
    Ok(SlidePath { id, samples })
}
