//! Temoto：從玩家手元影片學來的左右手修正模型（雙向 GRU）。
//!
//! 流程：V3 先求解 → 依譜面與 V3 的手算出每顆音符的特徵 → 模型估每顆用右手的機率
//! → 以模型的手當限制重新求解（照標註求解的同一套機制），保證動作做得到。
//!
//! 特徵與網路必須和訓練程式（J:\MaiMotionML\build_data.py、train.py）完全一致；
//! `tests/temoto.rs` 用 PyTorch 算好的參考機率比對。
use crate::model::*;
use crate::scoring_v3::TuningV3;
use crate::solver::{self, HandRules};
use std::collections::HashMap;
use std::sync::OnceLock;

/// 特徵格式版本；模型檔記錄的版本不同就拒絕載入。
pub const FEATURE_VERSION: u32 = 1;
const FEATURES: usize = 31;
/// 特徵 31 維＋V3 的手（接觸、滑行各 2 維：有沒有、左 −1／右 +1）。
const INPUTS: usize = FEATURES + 4;
/// 鏡像時 x 取負號的欄位：x、群重心 x、與前群 dx、終點 x、中點 x×2。
const X_COLS: [usize; 6] = [7, 17, 18, 20, 22, 24];
/// 群內由左到右的排名，鏡像時變成 1 − rank。
const RANK_COL: usize = 15;

pub const MODEL_IDS: [&str; 2] = ["temoto-1-tokiwa", "temoto-1-akane"];

struct Linear {
    w: Vec<f32>,
    b: Vec<f32>,
    inp: usize,
    out: usize,
}
impl Linear {
    fn apply(&self, x: &[f32], y: &mut [f32]) {
        for (o, out) in y.iter_mut().enumerate().take(self.out) {
            *out = self.b[o] + dot(&self.w[o * self.inp..(o + 1) * self.inp], x);
        }
    }
}

/// PyTorch GRU 單一方向：權重列依 r、z、n 排列。
struct GruDir {
    w_ih: Vec<f32>,
    w_hh: Vec<f32>,
    b_ih: Vec<f32>,
    b_hh: Vec<f32>,
    inp: usize,
}

struct Net {
    inp: Linear,
    /// 每層 [順向, 逆向]。
    layers: Vec<[GruDir; 2]>,
    hid: Linear,
    head: Linear,
    h: usize,
}

pub struct TemotoModel {
    pub name: String,
    tuning: Option<&'static TuningV3>,
    mu: Vec<f32>,
    sd: Vec<f32>,
    nets: Vec<Net>,
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    let mut s = [0f32; 8];
    let chunks = a.len() / 8;
    for i in 0..chunks {
        for k in 0..8 {
            s[k] += a[i * 8 + k] * b[i * 8 + k];
        }
    }
    let mut t: f32 = s.iter().sum();
    for i in chunks * 8..a.len() {
        t += a[i] * b[i];
    }
    t
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

/// PyTorch 預設的 GELU（erf 版）。
fn gelu(x: f32) -> f32 {
    let x = x as f64;
    (0.5 * x * (1.0 + erf(x / std::f64::consts::SQRT_2))) as f32
}

/// erf：Numerical Recipes 的 erfc Chebyshev 逼近（誤差 < 1.2e-7），對 f32 推論足夠。
fn erf(x: f64) -> f64 {
    let t = 1.0 / (1.0 + 0.5 * x.abs());
    let y = 1.0
        - t * (-x * x - 1.26551223
            + t * (1.00002368
                + t * (0.37409196
                    + t * (0.09678418
                        + t * (-0.18628806
                            + t * (0.27886807
                                + t * (-1.13520398
                                    + t * (1.48851587 + t * (-0.82215223 + t * 0.17087277)))))))))
            .exp();
    if x >= 0.0 {
        y
    } else {
        -y
    }
}

fn f16_to_f32(h: u16) -> f32 {
    let sign = ((h >> 15) as u32) << 31;
    let exp = ((h >> 10) & 0x1f) as u32;
    let frac = (h & 0x3ff) as u32;
    let bits = if exp == 0 {
        if frac == 0 {
            sign
        } else {
            // 次正規數
            let mut e = 127 - 15 + 1;
            let mut f = frac;
            while f & 0x400 == 0 {
                f <<= 1;
                e -= 1;
            }
            sign | ((e as u32) << 23) | ((f & 0x3ff) << 13)
        }
    } else if exp == 0x1f {
        sign | 0x7f80_0000 | (frac << 13)
    } else {
        sign | ((exp + 127 - 15) << 23) | (frac << 13)
    };
    f32::from_bits(bits)
}

impl TemotoModel {
    /// 讀 `.temoto`：b"TEMOTO1\n"、u32 標頭長度、JSON 標頭、f16 權重。
    pub fn load(bytes: &[u8]) -> Result<Self, String> {
        let magic = b"TEMOTO1\n";
        if bytes.len() < 12 || &bytes[..8] != magic {
            return Err("不是 Temoto 模型檔".into());
        }
        let len = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        let header: serde_json::Value = serde_json::from_slice(&bytes[12..12 + len])
            .map_err(|e| format!("模型標頭無法解析：{e}"))?;
        if header["featureVersion"].as_u64() != Some(FEATURE_VERSION as u64) {
            return Err(format!(
                "模型需要第 {} 版特徵，這個版本是第 {FEATURE_VERSION} 版",
                header["featureVersion"]
            ));
        }
        let data = &bytes[12 + len..];
        let weights: Vec<f32> = data
            .chunks_exact(2)
            .map(|c| f16_to_f32(u16::from_le_bytes([c[0], c[1]])))
            .collect();
        let floats = |v: &serde_json::Value| -> Vec<f32> {
            v.as_array()
                .map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect())
                .unwrap_or_default()
        };
        let h = header["hidden"].as_u64().unwrap_or(128) as usize;
        let layers = header["layers"].as_u64().unwrap_or(2) as usize;
        let mut nets = vec![];
        for table in header["nets"].as_array().ok_or("模型沒有網路")? {
            let tensor = |name: &str| -> Result<(Vec<f32>, Vec<usize>), String> {
                let entry = &table[name];
                let start = entry[0].as_u64().ok_or(format!("缺少 {name}"))? as usize;
                let shape: Vec<usize> = entry[1]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_u64().unwrap() as usize)
                    .collect();
                let size: usize = shape.iter().product();
                Ok((
                    weights
                        .get(start..start + size)
                        .ok_or("權重長度不符")?
                        .to_vec(),
                    shape,
                ))
            };
            let linear = |prefix: &str| -> Result<Linear, String> {
                let (w, shape) = tensor(&format!("{prefix}.weight"))?;
                let (b, _) = tensor(&format!("{prefix}.bias"))?;
                Ok(Linear {
                    w,
                    b,
                    out: shape[0],
                    inp: shape[1],
                })
            };
            let gru = |layer: usize, reverse: bool| -> Result<GruDir, String> {
                let suffix = format!("l{layer}{}", if reverse { "_reverse" } else { "" });
                let (w_ih, shape) = tensor(&format!("rnn.weight_ih_{suffix}"))?;
                let (w_hh, _) = tensor(&format!("rnn.weight_hh_{suffix}"))?;
                let (b_ih, _) = tensor(&format!("rnn.bias_ih_{suffix}"))?;
                let (b_hh, _) = tensor(&format!("rnn.bias_hh_{suffix}"))?;
                Ok(GruDir {
                    w_ih,
                    w_hh,
                    b_ih,
                    b_hh,
                    inp: shape[1],
                })
            };
            let mut net_layers = vec![];
            for l in 0..layers {
                net_layers.push([gru(l, false)?, gru(l, true)?]);
            }
            let inp = linear("inp.0")?;
            if inp.inp != INPUTS {
                return Err(format!("模型輸入是 {} 維，程式產生 {INPUTS} 維", inp.inp));
            }
            nets.push(Net {
                inp,
                layers: net_layers,
                hid: linear("out.0")?,
                head: linear("out.3")?,
                h,
            });
        }
        let tuning = match &header["tuning"] {
            serde_json::Value::Null => None,
            t => Some(&*Box::leak(Box::new(
                serde_json::from_value::<TuningV3>(t.clone())
                    .map_err(|e| format!("模型的 V3 權重無法解析：{e}"))?,
            ))),
        };
        Ok(Self {
            name: header["name"].as_str().unwrap_or("temoto").into(),
            tuning,
            mu: floats(&header["mu"]),
            sd: floats(&header["sd"]),
            nets,
        })
    }

    /// 內建模型（依 id 取得，第一次使用時解壓）。
    pub fn builtin(id: &str) -> Option<&'static TemotoModel> {
        static TOKIWA: OnceLock<TemotoModel> = OnceLock::new();
        static AKANE: OnceLock<TemotoModel> = OnceLock::new();
        match id {
            "temoto-1-tokiwa" => Some(TOKIWA.get_or_init(|| {
                Self::load(include_bytes!("../models/temoto-1-tokiwa.temoto"))
                    .expect("內建模型 tokiwa")
            })),
            "temoto-1-akane" => Some(AKANE.get_or_init(|| {
                Self::load(include_bytes!("../models/temoto-1-akane.temoto"))
                    .expect("內建模型 akane")
            })),
            _ => None,
        }
    }

    pub fn tuning(&self) -> Option<&'static TuningV3> {
        self.tuning
    }

    /// 每顆音符（依 `features` 的排序）用右手的機率：[接觸, 滑行]。三個 seed × 鏡像平均。
    pub fn predict(&self, input: &Features) -> Vec<[f32; 2]> {
        let n = input.x.len();
        if n == 0 {
            return vec![];
        }
        let runs: Vec<(usize, bool)> = (0..self.nets.len())
            .flat_map(|k| [(k, false), (k, true)])
            .collect();
        let outputs: Vec<Vec<[f32; 2]>> = std::thread::scope(|scope| {
            let handles: Vec<_> = runs
                .iter()
                .map(|&(k, flipped)| {
                    scope.spawn(move || {
                        let x = self.normalized(input, flipped);
                        let p = self.nets[k].forward(&x);
                        p.into_iter()
                            .map(|[a, b]| if flipped { [1.0 - a, 1.0 - b] } else { [a, b] })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let m = outputs.len() as f32;
        (0..n)
            .map(|i| {
                let mut s = [0f32; 2];
                for o in &outputs {
                    s[0] += o[i][0];
                    s[1] += o[i][1];
                }
                [s[0] / m, s[1] / m]
            })
            .collect()
    }

    fn normalized(&self, input: &Features, flipped: bool) -> Vec<f32> {
        let enc = |v: i8| -> [f32; 2] {
            if v < 0 {
                [0.0, 0.0]
            } else {
                let v = if flipped { 1 - v } else { v };
                [1.0, 2.0 * v as f32 - 1.0]
            }
        };
        let mut out = Vec::with_capacity(input.x.len() * INPUTS);
        for (i, row) in input.x.iter().enumerate() {
            let mut r = [0f32; INPUTS];
            r[..FEATURES].copy_from_slice(row);
            if flipped {
                for c in X_COLS {
                    r[c] = -r[c];
                }
                r[RANK_COL] = 1.0 - r[RANK_COL];
            }
            let [a, b] = enc(input.v3_hand[i]);
            let [c, d] = enc(input.v3_track[i]);
            r[FEATURES..].copy_from_slice(&[a, b, c, d]);
            for (k, v) in r.iter().enumerate() {
                out.push((v - self.mu[k]) / self.sd[k]);
            }
        }
        out
    }
}

impl Net {
    fn forward(&self, x: &[f32]) -> Vec<[f32; 2]> {
        let n = x.len() / INPUTS;
        let h = self.h;
        // 輸入層
        let mut seq = vec![0f32; n * h];
        for t in 0..n {
            self.inp.apply(
                &x[t * INPUTS..(t + 1) * INPUTS],
                &mut seq[t * h..(t + 1) * h],
            );
            for v in &mut seq[t * h..(t + 1) * h] {
                *v = gelu(*v);
            }
        }
        let mut width = h;
        for layer in &self.layers {
            let mut next = vec![0f32; n * 2 * h];
            for (d, dir) in layer.iter().enumerate() {
                // 先算所有時刻的輸入投影
                let mut proj = vec![0f32; n * 3 * h];
                for t in 0..n {
                    let xt = &seq[t * width..(t + 1) * width];
                    for o in 0..3 * h {
                        proj[t * 3 * h + o] =
                            dir.b_ih[o] + dot(&dir.w_ih[o * dir.inp..(o + 1) * dir.inp], xt);
                    }
                }
                let mut state = vec![0f32; h];
                let mut rec = vec![0f32; 3 * h];
                for step in 0..n {
                    let t = if d == 0 { step } else { n - 1 - step };
                    for (o, r) in rec.iter_mut().enumerate() {
                        *r = dir.b_hh[o] + dot(&dir.w_hh[o * h..(o + 1) * h], &state);
                    }
                    let p = &proj[t * 3 * h..(t + 1) * 3 * h];
                    for j in 0..h {
                        let r = sigmoid(p[j] + rec[j]);
                        let z = sigmoid(p[h + j] + rec[h + j]);
                        let cand = (p[2 * h + j] + r * rec[2 * h + j]).tanh();
                        state[j] = (1.0 - z) * cand + z * state[j];
                    }
                    next[t * 2 * h + d * h..t * 2 * h + (d + 1) * h].copy_from_slice(&state);
                }
            }
            seq = next;
            width = 2 * h;
        }
        let mut hidden = vec![0f32; self.hid.out];
        let mut logits = [0f32; 2];
        (0..n)
            .map(|t| {
                self.hid
                    .apply(&seq[t * width..(t + 1) * width], &mut hidden);
                for v in &mut hidden {
                    *v = gelu(*v);
                }
                self.head.apply(&hidden, &mut logits);
                [sigmoid(logits[0]), sigmoid(logits[1])]
            })
            .collect()
    }
}

/// 模型輸入：音符依（時間、x）排序後的特徵，以及 V3 給的手（−1 沒有、0 左、1 右）。
pub struct Features {
    /// 排序後第 i 顆對應的 chart.notes 索引。
    pub order: Vec<usize>,
    pub x: Vec<[f32; FEATURES]>,
    pub v3_hand: Vec<i8>,
    pub v3_track: Vec<i8>,
}

fn button_xy(b: u8) -> (f64, f64) {
    let a = std::f64::consts::FRAC_PI_2 - (b as f64 - 0.5) * std::f64::consts::FRAC_PI_4;
    (a.cos(), -a.sin())
}

fn lg(dt: f64) -> f64 {
    (dt.max(0.0) * 10.0).ln_1p()
}

/// 與 build_data.py 的 song_features 逐項對應。
pub fn features(chart: &Chart, solution: Option<&Solution>) -> Features {
    let mut order: Vec<usize> = (0..chart.notes.len()).collect();
    order.sort_by(|&a, &b| {
        let (na, nb) = (&chart.notes[a], &chart.notes[b]);
        na.time_seconds
            .partial_cmp(&nb.time_seconds)
            .unwrap()
            .then(na.position.x.partial_cmp(&nb.position.x).unwrap())
    });
    let notes: Vec<&Note> = order.iter().map(|&i| &chart.notes[i]).collect();
    let paths: HashMap<&str, &SlidePath> = chart.paths.iter().map(|p| (p.id.as_str(), p)).collect();
    let mut v3: HashMap<&str, HashMap<&str, i8>> = HashMap::new();
    if let Some(s) = solution {
        for a in &s.assignments {
            v3.entry(a.note_id.as_str())
                .or_default()
                .insert(a.part.as_str(), if a.hand == Hand::R { 1 } else { 0 });
        }
    }
    let mut bxy: HashMap<u8, (f64, f64)> = HashMap::new();
    for n in &notes {
        if n.button != 0 {
            bxy.entry(n.button).or_insert((n.position.x, n.position.y));
        }
    }
    for b in 1..=8 {
        bxy.entry(b).or_insert(button_xy(b));
    }
    // 同時（±1 ms，與前一顆比）分組
    let mut groups = vec![];
    let mut gid: i64 = -1;
    let mut last: Option<f64> = None;
    for n in &notes {
        let t = n.time_seconds;
        if last.is_none_or(|l| t - l > 0.001) {
            gid += 1;
        }
        groups.push(gid as usize);
        last = Some(t);
    }
    let gcount = groups.last().map_or(0, |g| g + 1);
    let mut members: Vec<Vec<usize>> = vec![vec![]; gcount];
    for (j, &g) in groups.iter().enumerate() {
        members[g].push(j);
    }
    let gtime: Vec<f64> = members.iter().map(|m| notes[m[0]].time_seconds).collect();
    let centroid = |m: &Vec<usize>, f: fn(&Note) -> f64| {
        m.iter().map(|&j| f(notes[j])).sum::<f64>() / m.len() as f64
    };
    let gcx: Vec<f64> = members
        .iter()
        .map(|m| centroid(m, |n| n.position.x))
        .collect();
    let gcy: Vec<f64> = members
        .iter()
        .map(|m| centroid(m, |n| n.position.y))
        .collect();

    let mut out = Features {
        order: order.clone(),
        x: vec![],
        v3_hand: vec![],
        v3_track: vec![],
    };
    let mut last_by_btn: HashMap<u8, f64> = HashMap::new();
    let kinds = ["tap", "hold", "touch", "touchHold", "slide"];
    for (i, n) in notes.iter().enumerate() {
        let t = n.time_seconds;
        let (x, y) = (n.position.x, n.position.y);
        let g = groups[i];
        let m = &members[g];
        let mx: Vec<f64> = m.iter().map(|&j| notes[j].position.x).collect();
        let rank = if m.len() > 1 {
            let mut sorted = mx.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
            sorted.iter().position(|v| *v == x).unwrap() as f64 / (m.len() - 1) as f64
        } else {
            0.5
        };
        let dtp = if g > 0 { t - gtime[g - 1] } else { 5.0 };
        let dtn = if g + 1 < gcount {
            gtime[g + 1] - t
        } else {
            5.0
        };
        let (pcx, pcy) = if g > 0 {
            (gcx[g - 1], gcy[g - 1])
        } else {
            (0.0, 0.0)
        };
        let btn = n.button;
        let same_key_dt = if btn != 0 {
            last_by_btn.get(&btn).map_or(5.0, |l| t - l)
        } else {
            5.0
        };
        if btn != 0 {
            last_by_btn.insert(btn, t);
        }
        let end = if n.end_seconds != 0.0 {
            n.end_seconds
        } else {
            t
        };
        let dur = end - t;
        let is_slide = n.kind == "slide";
        let (mut ex, mut ey) = (0.0, 0.0);
        let mut mid = [0.0; 4];
        let (mut slen, mut sdur, mut sdelay) = (0.0, 0.0, 0.0);
        if is_slide {
            if let Some(p) = n.path_id.as_deref().and_then(|id| paths.get(id)) {
                if p.end_button != 0 {
                    (ex, ey) = bxy.get(&p.end_button).copied().unwrap_or((0.0, 0.0));
                }
                let s = &p.samples;
                if !s.is_empty() {
                    (ex, ey) = (s[s.len() - 1].x, s[s.len() - 1].y);
                    let (a1, a2) = (&s[s.len() / 3], &s[2 * s.len() / 3]);
                    mid = [a1.x, a1.y, a2.x, a2.y];
                    slen = s
                        .windows(2)
                        .map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y))
                        .sum();
                }
                let or_t = |v: Option<f64>| v.filter(|v| *v != 0.0).unwrap_or(t);
                let (ms, me) = (or_t(n.motion_start), or_t(n.motion_end));
                sdelay = ms - t;
                sdur = me - ms;
            }
        }
        let span = if m.len() > 1 {
            mx.iter().cloned().fold(f64::MIN, f64::max)
                - mx.iter().cloned().fold(f64::MAX, f64::min)
        } else {
            0.0
        };
        let mut f: Vec<f64> = kinds
            .iter()
            .map(|k| if n.kind == *k { 1.0 } else { 0.0 })
            .collect();
        f.extend([
            if n.has_head { 1.0 } else { 0.0 },
            if n.touch_area.as_deref().is_some_and(|a| !a.is_empty()) {
                1.0
            } else {
                0.0
            },
            x,
            y,
            x.hypot(y),
            lg(dtp),
            lg(dtn),
            lg(same_key_dt),
            lg(dur),
            m.len() as f64,
            rank,
            span,
            mx.iter().sum::<f64>() / mx.len() as f64,
            x - pcx,
            y - pcy,
            ex,
            ey,
            mid[0],
            mid[1],
            mid[2],
            mid[3],
            slen,
            lg(sdelay),
            lg(sdur),
            if n.modifiers.break_note { 1.0 } else { 0.0 },
            if n.modifiers.ex { 1.0 } else { 0.0 },
        ]);
        let mut row = [0f32; FEATURES];
        for (k, v) in f.iter().enumerate() {
            row[k] = *v as f32;
        }
        out.x.push(row);
        let a = v3.get(n.id.as_str());
        out.v3_hand.push(
            a.and_then(|a| a.get("contact").or(a.get("head")).copied())
                .unwrap_or(-1),
        );
        out.v3_track
            .push(a.and_then(|a| a.get("slide").copied()).unwrap_or(-1));
    }
    out
}

/// 一條模型給的手部限制。
struct Constraint {
    note_id: String,
    time: f64,
    track: bool,
    hand: Hand,
    /// 和 V3 的手不同（真正的修正）。
    flipped: bool,
    active: bool,
}

fn build_rules(constraints: &[Constraint]) -> HandRules {
    let mut rules = HandRules::default();
    for k in constraints.iter().filter(|k| k.active) {
        if k.track {
            rules
                .track
                .insert(k.note_id.clone(), vec![(f64::NEG_INFINITY, k.hand)]);
        } else {
            rules.contact.insert(k.note_id.clone(), k.hand);
        }
    }
    rules
}

/// 放寬次數上限：每次在走不下去的時刻附近拿掉一些限制。
const MAX_RELAX: usize = 24;

/// Temoto 求解：V3（或模型附帶的權重）→ 模型修正 → 照修正後的手重新求解。
/// 照修正走不下去時，在卡住的時刻附近逐步拿掉限制（先拿掉修正的，再拿掉全部）再試；
/// 仍不行就回傳 V3 的結果並加註。
pub fn solve(
    chart: &Chart,
    c: &SolverConfig,
    model: &TemotoModel,
) -> Result<Vec<Solution>, Diagnostic> {
    let run = || -> Result<Vec<Solution>, Diagnostic> {
        let base = solver::solve(chart, c)?;
        let Some(top) = base.first() else {
            return Ok(base);
        };
        let input = features(chart, Some(top));
        let probs = model.predict(&input);
        let mut constraints = vec![];
        for (i, &ni) in input.order.iter().enumerate() {
            let note = &chart.notes[ni];
            let hand = |p: f32| if p > 0.5 { Hand::R } else { Hand::L };
            let mut push = |track: bool, p: f32, v3: i8| {
                let h = hand(p);
                constraints.push(Constraint {
                    note_id: note.id.clone(),
                    time: note.time_seconds,
                    track,
                    hand: h,
                    flipped: (h == Hand::R) != (v3 == 1),
                    active: true,
                });
            };
            if input.v3_hand[i] >= 0 {
                push(false, probs[i][0], input.v3_hand[i]);
            }
            // WiFi（兩手 2+1）與 V3 中途換手的 Slide 不限制滑行手
            let wifi = note
                .path_id
                .as_deref()
                .and_then(|id| chart.paths.iter().find(|p| p.id == id))
                .is_some_and(|p| p.branches.len() == 2);
            let handover = top
                .assignments
                .iter()
                .filter(|a| a.note_id == note.id && a.part == "slide")
                .count()
                > 1;
            if input.v3_track[i] >= 0 && !wifi && !handover {
                push(true, probs[i][1], input.v3_track[i]);
            }
        }
        let mut last_fail: Option<f64> = None;
        let mut strikes = 0;
        for _ in 0..=MAX_RELAX {
            match solver::solve_constrained(chart, c, &build_rules(&constraints)) {
                Ok(mut solutions) => {
                    let applied = constraints.iter().filter(|k| k.active && k.flipped).count();
                    let dropped = constraints
                        .iter()
                        .filter(|k| !k.active && k.flipped)
                        .count();
                    let note = match (applied, dropped) {
                        (0, 0) => "Temoto-1：檢查過，沒有需要修改的手順。".to_string(),
                        (a, 0) => format!("Temoto-1：修改了 {a} 個手順。"),
                        (a, d) => format!(
                            "Temoto-1：修改了 {a} 個手順；另有 {d} 個修改動作上做不到，沿用 V3。"
                        ),
                    };
                    for s in &mut solutions {
                        s.warnings.insert(0, note.clone());
                    }
                    return Ok(solutions);
                }
                Err(e) if e.code == "annotation_infeasible" && e.time_seconds.is_some() => {
                    let t = e.time_seconds.unwrap();
                    strikes = if last_fail.is_some_and(|l| (l - t).abs() < 1e-6) {
                        strikes + 1
                    } else {
                        0
                    };
                    last_fail = Some(t);
                    // 第一次只拿掉卡住處前 0.5 秒內的修正；同一處再卡住就擴大範圍、連沿用 V3 的限制也拿掉
                    let back = 0.5 * (1 + strikes) as f64;
                    let all = strikes > 0;
                    let mut removed = 0;
                    for k in constraints
                        .iter_mut()
                        .filter(|k| k.active && k.time >= t - back && k.time <= t + 0.05)
                    {
                        if all || k.flipped {
                            k.active = false;
                            removed += 1;
                        }
                    }
                    if removed == 0 && strikes > 3 {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let mut base = base;
        for s in &mut base {
            s.warnings.insert(
                0,
                "Temoto-1 的修正在這份譜面走不下去，改用 V3 的結果。".into(),
            );
        }
        Ok(base)
    };
    match model.tuning() {
        Some(t) => crate::scoring_v3::with_tuning(t, run),
        None => run(),
    }
}
