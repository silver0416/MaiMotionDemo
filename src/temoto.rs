//! Temoto：從玩家手元影片學來的左右手修正模型（雙向 GRU）。
//!
//! 流程：V3 先求解 → 依譜面與 V3 的手算出每顆音符的特徵 → 模型估每顆用右手的機率
//! → 以模型的手當限制重新求解（照標註求解的同一套機制），保證動作做得到。
//!
//! 兩種架構：
//! - 逐顆（tokiwa、akane）：整首一次跑完逐顆 GRU。
//! - 分組（ruri）：逐顆 GRU → 每個自動分組取平均 → 組與組之間的 GRU → 組的打法
//!   （全左、全右、左起交替、右起交替、自由）與逐顆的手；有規則的分組整組套用修正。
//!
//! 特徵與網路必須和訓練程式（J:\MaiMotionML\build_data.py、train.py；分組架構在
//! J:\MaiMotionML\g1）完全一致；`tests/temoto.rs` 用 PyTorch 算好的參考機率比對。
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

/// 內建模型。rurikon（瑠璃紺）是 ruri 的網路配 V4 權重，ayame（菖蒲）是以 V4 基座重訓的分組架構
/// （2026-10-30 使用者選入 App；tokiwa、akane、ruri 收進前端的「顯示更多模型」）。
pub const MODEL_IDS: [&str; 5] = [
    "temoto-1-tokiwa",
    "temoto-1-akane",
    "temoto-1-ruri",
    "temoto-1-rurikon",
    "temoto-1-ayame",
];

/// 分組特徵的格式版本（分組架構的模型檔另外記錄）。
pub const GROUP_FEATURE_VERSION: u32 = 1;
/// 分組的種類：同鍵、階梯、兩顆、其他、含 Touch。
const GROUP_TYPES: usize = 5;
/// 每顆的分組特徵：在組內、種類、含星星、classify 規則、auto 規則、組內位置、是否第一顆、log(1+顆數)。
const G_COLS: usize = 1 + GROUP_TYPES + 1 + 3 + 4 + 3;
/// 每組的描述：種類、含星星、classify 規則、auto 規則、log(1+顆數)、組長、和前一組開頭的間隔。
const GD_COLS: usize = GROUP_TYPES + 1 + 3 + 4 + 3;
/// 打法固定時，單顆和打法不同的機率（和訓練相同）。
const PATTERN_EPS: f32 = 0.05;

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
    /// 分組架構才有。
    group: Option<GroupLayers>,
    hid: Linear,
    head: Linear,
    h: usize,
}

/// 分組架構多出來的層：組的輸入、組與組之間的雙向 GRU、組的打法頭。
struct GroupLayers {
    gin: Linear,
    grnn: [GruDir; 2],
    pat_hid: Linear,
    pat_out: Linear,
    gh: usize,
}

pub struct TemotoModel {
    pub name: String,
    tuning: Option<&'static TuningV3>,
    /// 和 V3 的手不同時，模型的把握（較大那一邊的機率）要到這個值才改，不到就照 V3。
    /// 模型檔標頭的 `flipAt`，沒寫為 0.5。
    flip_at: f32,
    /// 基座（V3）的手是不是輸入：模型檔標頭 `input` 為 `"none"` 的純模型不看，那兩欄一律當成
    /// 「沒有」；這時每顆都照模型判斷（`flipAt` 0.5），基座只負責算動作與確認做得到。
    uses_base: bool,
    /// 每顆的輸入維度：逐顆架構 `INPUTS`，分組架構另加 `G_COLS`。
    inputs: usize,
    /// 分組架構：推論視窗長度（訓練時的切段長度，視窗重疊一半）。逐顆架構為 None，整首一次跑完。
    window: Option<usize>,
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
        let grouped = match header["arch"].as_str().unwrap_or("note") {
            "note" => false,
            "group" => true,
            other => return Err(format!("不支援的模型架構：{other}")),
        };
        if grouped && header["groupFeatures"].as_u64() != Some(GROUP_FEATURE_VERSION as u64) {
            return Err(format!(
                "模型需要第 {} 版分組特徵，這個版本是第 {GROUP_FEATURE_VERSION} 版",
                header["groupFeatures"]
            ));
        }
        let inputs = if grouped { INPUTS + G_COLS } else { INPUTS };
        let window = match header["window"].as_u64() {
            Some(w) if grouped && w >= 2 => Some(w as usize),
            None if !grouped => None,
            _ => return Err("分組模型需要 2 以上的推論視窗".into()),
        };
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
            let gru = |rnn: &str, layer: usize, reverse: bool| -> Result<GruDir, String> {
                let suffix = format!("l{layer}{}", if reverse { "_reverse" } else { "" });
                let (w_ih, shape) = tensor(&format!("{rnn}.weight_ih_{suffix}"))?;
                let (w_hh, _) = tensor(&format!("{rnn}.weight_hh_{suffix}"))?;
                let (b_ih, _) = tensor(&format!("{rnn}.bias_ih_{suffix}"))?;
                let (b_hh, _) = tensor(&format!("{rnn}.bias_hh_{suffix}"))?;
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
                net_layers.push([gru("rnn", l, false)?, gru("rnn", l, true)?]);
            }
            let inp = linear("inp.0")?;
            if inp.inp != inputs {
                return Err(format!("模型輸入是 {} 維，程式產生 {inputs} 維", inp.inp));
            }
            let group = if grouped {
                Some(GroupLayers {
                    gin: linear("gin.0")?,
                    grnn: [gru("grnn", 0, false)?, gru("grnn", 0, true)?],
                    pat_hid: linear("pat.0")?,
                    pat_out: linear("pat.2")?,
                    gh: header["groupHidden"].as_u64().unwrap_or(64) as usize,
                })
            } else {
                None
            };
            nets.push(Net {
                inp,
                layers: net_layers,
                group,
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
        let (mu, sd) = (floats(&header["mu"]), floats(&header["sd"]));
        if mu.len() != inputs || sd.len() != inputs {
            return Err(format!("模型的正規化參數不是 {inputs} 維"));
        }
        Ok(Self {
            name: header["name"].as_str().unwrap_or("temoto").into(),
            tuning,
            flip_at: header["flipAt"].as_f64().map_or(0.5, |v| v as f32),
            uses_base: header["input"].as_str() != Some("none"),
            inputs,
            window,
            mu,
            sd,
            nets,
        })
    }

    /// 內建模型（依 id 取得，第一次使用時解壓）。
    ///
    /// 改動門檻依四份真人標註的掃描決定：tokiwa 在 0.6 時合計多對最多，把握不到 0.6 的修改
    /// 改對與改錯差不多；akane 在 0.5 時最多。ruri（分組架構，基座 V3.2）沿用 0.5，
    /// rurikon、ayame（分組架構，基座 V4）也是 0.5。
    pub fn builtin(id: &str) -> Option<&'static TemotoModel> {
        static TOKIWA: OnceLock<TemotoModel> = OnceLock::new();
        static AKANE: OnceLock<TemotoModel> = OnceLock::new();
        static RURI: OnceLock<TemotoModel> = OnceLock::new();
        static RURIKON: OnceLock<TemotoModel> = OnceLock::new();
        static AYAME: OnceLock<TemotoModel> = OnceLock::new();
        match id {
            "temoto-1-tokiwa" => Some(TOKIWA.get_or_init(|| {
                Self {
                    flip_at: 0.6,
                    ..Self::load(include_bytes!("../models/temoto-1-tokiwa.temoto"))
                        .expect("內建模型 tokiwa")
                }
            })),
            "temoto-1-akane" => Some(AKANE.get_or_init(|| {
                Self::load(include_bytes!("../models/temoto-1-akane.temoto"))
                    .expect("內建模型 akane")
            })),
            "temoto-1-ruri" => Some(RURI.get_or_init(|| {
                Self::load(include_bytes!("../models/temoto-1-ruri.temoto")).expect("內建模型 ruri")
            })),
            "temoto-1-rurikon" => Some(RURIKON.get_or_init(|| {
                Self::load(include_bytes!("../models/temoto-1-rurikon.temoto"))
                    .expect("內建模型 rurikon")
            })),
            "temoto-1-ayame" => Some(AYAME.get_or_init(|| {
                Self::load(include_bytes!("../models/temoto-1-ayame.temoto"))
                    .expect("內建模型 ayame")
            })),
            _ => None,
        }
    }

    /// 分組架構：模型讀分組特徵，修正時有規則的自動分組整組套用。
    pub fn grouped(&self) -> bool {
        self.window.is_some()
    }

    pub fn tuning(&self) -> Option<&'static TuningV3> {
        self.tuning
    }

    /// 基座在提醒文字裡的名稱：附帶的權重和哪個 V3 變體相同就用它的名字（V3.1、V3.2、V4），
    /// 沒附權重是原始 V3，都對不上時只說「基座」。
    pub fn base_label(&self) -> String {
        let Some(tuning) = self.tuning else {
            return "V3".into();
        };
        let Ok(value) = serde_json::to_value(tuning) else {
            return "基座".into();
        };
        crate::scoring_v3::V3_VARIANTS
            .iter()
            .find(|id| {
                crate::scoring_v3::variant_tuning(id)
                    .and_then(|v| serde_json::to_value(v).ok())
                    .as_ref()
                    == Some(&value)
            })
            .map_or_else(|| "基座".into(), |id| id.to_uppercase())
    }

    pub fn flip_at(&self) -> f32 {
        self.flip_at
    }

    /// 每顆音符（依 `features` 的排序）用右手的機率：[接觸, 滑行]。各 seed × 左右鏡像平均。
    /// 分組架構照訓練時切成重疊一半的視窗，每顆取離視窗中心最近的那一段。
    pub fn predict(&self, input: &Features) -> Vec<[f32; 2]> {
        let n = input.x.len();
        if n == 0 {
            return vec![];
        }
        let normalized = [self.normalized(input, false), self.normalized(input, true)];
        let spans: Vec<(usize, usize)> = match self.window {
            Some(win) if n > win => (0..(n - win / 2).max(1))
                .step_by(win / 2)
                .map(|a| (a, (a + win).min(n)))
                .collect(),
            _ => vec![(0, n)],
        };
        let runs: Vec<(usize, usize, bool)> = (0..spans.len())
            .flat_map(|s| (0..self.nets.len()).flat_map(move |k| [(s, k, false), (s, k, true)]))
            .collect();
        let width = self.inputs;
        let outputs: Vec<Vec<[f32; 2]>> = std::thread::scope(|scope| {
            let handles: Vec<_> = runs
                .iter()
                .map(|&(s, k, flipped)| {
                    let (a, b) = spans[s];
                    let x = &normalized[usize::from(flipped)][a * width..b * width];
                    scope.spawn(move || {
                        let mut p = match self.window {
                            // 和訓練時一樣補零到整個視窗長度再跑：逐顆 GRU 的逆向會先經過補的零
                            Some(win) => {
                                let mut padded = vec![0f32; win * width];
                                padded[..x.len()].copy_from_slice(x);
                                let groups = WindowGroups::new(input, a, b);
                                self.nets[k].forward(&padded, width, Some(&groups))
                            }
                            None => self.nets[k].forward(x, width, None),
                        };
                        p.truncate(b - a);
                        p.into_iter()
                            .map(|[p, q]| if flipped { [1.0 - p, 1.0 - q] } else { [p, q] })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let per = 2 * self.nets.len();
        let mut out = vec![[0f32; 2]; n];
        let mut nearest = vec![f64::INFINITY; n];
        for (s, &(a, b)) in spans.iter().enumerate() {
            let center = (a + b) as f64 / 2.0;
            for j in a..b {
                let d = (j as f64 - center).abs();
                if d < nearest[j] {
                    nearest[j] = d;
                    let mut sum = [0f32; 2];
                    for o in &outputs[s * per..(s + 1) * per] {
                        sum[0] += o[j - a][0];
                        sum[1] += o[j - a][1];
                    }
                    out[j] = [sum[0] / per as f32, sum[1] / per as f32];
                }
            }
        }
        out
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
        let mut out = Vec::with_capacity(input.x.len() * self.inputs);
        let mut r = vec![0f32; self.inputs];
        for (i, row) in input.x.iter().enumerate() {
            r[..FEATURES].copy_from_slice(row);
            if flipped {
                for c in X_COLS {
                    r[c] = -r[c];
                }
                r[RANK_COL] = 1.0 - r[RANK_COL];
            }
            let (hand, track) = if self.uses_base {
                (input.v3_hand[i], input.v3_track[i])
            } else {
                (-1, -1)
            };
            let [a, b] = enc(hand);
            let [c, d] = enc(track);
            r[FEATURES..INPUTS].copy_from_slice(&[a, b, c, d]);
            // 分組特徵左右鏡像不變
            if self.inputs > INPUTS {
                r[INPUTS..].copy_from_slice(&input.g[i]);
            }
            for (k, v) in r.iter().enumerate() {
                out.push((v - self.mu[k]) / self.sd[k]);
            }
        }
        out
    }
}

/// 視窗 [a, b) 裡的組（依組的編號排序）：組在視窗內的音符（視窗內的位置）與組的描述，
/// 以及每顆所在的組、在組內是第奇數顆還是偶數顆（交替打法用）。
struct WindowGroups {
    members: Vec<Vec<usize>>,
    desc: Vec<[f32; GD_COLS]>,
    note_group: Vec<Option<usize>>,
    odd: Vec<bool>,
}

impl WindowGroups {
    fn new(input: &Features, a: usize, b: usize) -> Self {
        let mut ids: Vec<usize> = input.gid[a..b].iter().flatten().copied().collect();
        ids.sort_unstable();
        ids.dedup();
        let local: HashMap<usize, usize> = ids.iter().enumerate().map(|(k, &g)| (g, k)).collect();
        let mut members = vec![vec![]; ids.len()];
        let note_group: Vec<Option<usize>> = (a..b)
            .map(|j| {
                input.gid[j].map(|g| {
                    let k = local[&g];
                    members[k].push(j - a);
                    k
                })
            })
            .collect();
        Self {
            members,
            desc: ids.iter().map(|&g| input.gdesc[g]).collect(),
            note_group,
            odd: (a..b).map(|j| input.gpos[j] % 2 == 1).collect(),
        }
    }
}

/// 雙向 GRU 一層：輸入 n × width，輸出 n × 2h（每列 [順向, 逆向]）。
fn bigru(dirs: &[GruDir; 2], seq: &[f32], n: usize, width: usize, h: usize) -> Vec<f32> {
    let mut next = vec![0f32; n * 2 * h];
    for (d, dir) in dirs.iter().enumerate() {
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
    next
}

impl GroupLayers {
    /// 每組：組內音符的 GRU 輸出取平均、接上組的描述 → 組與組之間的雙向 GRU。
    /// 回傳每組的狀態（G × 2gh）與五種打法的機率。
    fn forward(
        &self,
        seq: &[f32],
        width: usize,
        groups: &WindowGroups,
    ) -> (Vec<f32>, Vec<[f32; 5]>) {
        let count = groups.members.len();
        let gw = self.gin.out;
        let mut g = vec![0f32; count * gw];
        let mut row = vec![0f32; width + GD_COLS];
        for (k, members) in groups.members.iter().enumerate() {
            row.fill(0.0);
            for &j in members {
                for (c, v) in row[..width].iter_mut().enumerate() {
                    *v += seq[j * width + c];
                }
            }
            let cnt = members.len().max(1) as f32;
            for v in &mut row[..width] {
                *v /= cnt;
            }
            row[width..].copy_from_slice(&groups.desc[k]);
            let out = &mut g[k * gw..(k + 1) * gw];
            self.gin.apply(&row, out);
            for v in out.iter_mut() {
                *v = gelu(*v);
            }
        }
        let states = bigru(&self.grnn, &g, count, gw, self.gh);
        let sw = 2 * self.gh;
        let mut hidden = vec![0f32; self.pat_hid.out];
        let mut logits = [0f32; 5];
        let patterns = (0..count)
            .map(|k| {
                self.pat_hid
                    .apply(&states[k * sw..(k + 1) * sw], &mut hidden);
                for v in &mut hidden {
                    *v = gelu(*v);
                }
                self.pat_out.apply(&hidden, &mut logits);
                let top = logits.iter().cloned().fold(f32::MIN, f32::max);
                let e = logits.map(|l| (l - top).exp());
                let sum: f32 = e.iter().sum();
                e.map(|v| v / sum)
            })
            .collect();
        (states, patterns)
    }
}

/// 組內音符用右手的機率：對五種打法（全左、全右、左起交替、右起交替、自由）取邊際。
/// 固定打法時單顆仍有 `PATTERN_EPS` 的機率和打法不同；自由打法用逐顆頭的機率。
fn pattern_marginal(pattern: &[f32; 5], odd: bool, free: f32) -> f32 {
    let alt_left = if odd { 1.0 } else { 0.0 };
    let fixed = [0.0, 1.0, alt_left, 1.0 - alt_left];
    let mut m = pattern[4] * free;
    for (pi, v) in pattern.iter().zip(fixed) {
        m += pi * (v * (1.0 - 2.0 * PATTERN_EPS) + PATTERN_EPS);
    }
    m
}

impl Net {
    /// `x`：n 顆、每顆 `inputs` 維（已正規化）。分組架構另給這段的組（`groups`）。
    fn forward(&self, x: &[f32], inputs: usize, groups: Option<&WindowGroups>) -> Vec<[f32; 2]> {
        let n = x.len() / inputs;
        let h = self.h;
        // 輸入層
        let mut seq = vec![0f32; n * h];
        for t in 0..n {
            self.inp.apply(
                &x[t * inputs..(t + 1) * inputs],
                &mut seq[t * h..(t + 1) * h],
            );
            for v in &mut seq[t * h..(t + 1) * h] {
                *v = gelu(*v);
            }
        }
        let mut width = h;
        for layer in &self.layers {
            seq = bigru(layer, &seq, n, width, h);
            width = 2 * h;
        }
        // 分組架構：每顆接上所在組的狀態與「在組內」旗標（不在組內為 0），組內的手對打法取邊際
        let grouped = self
            .group
            .as_ref()
            .zip(groups)
            .map(|(layers, groups)| (layers.forward(&seq, width, groups), groups));
        let extra = self.group.as_ref().map_or(0, |g| 2 * g.gh + 1);
        let mut row = vec![0f32; width + extra];
        let mut hidden = vec![0f32; self.hid.out];
        let mut logits = [0f32; 2];
        (0..n)
            .map(|t| {
                row[..width].copy_from_slice(&seq[t * width..(t + 1) * width]);
                row[width..].fill(0.0);
                let mut pattern = None;
                if let Some(((states, patterns), groups)) = &grouped {
                    // 補零的列不在任何組裡
                    if let Some(k) = groups.note_group.get(t).copied().flatten() {
                        let sw = extra - 1;
                        row[width..width + sw].copy_from_slice(&states[k * sw..(k + 1) * sw]);
                        row[width + sw] = 1.0;
                        pattern = Some((&patterns[k], groups.odd[t]));
                    }
                }
                self.hid.apply(&row, &mut hidden);
                for v in &mut hidden {
                    *v = gelu(*v);
                }
                self.head.apply(&hidden, &mut logits);
                let free = sigmoid(logits[0]);
                let hand = match pattern {
                    Some((p, odd)) => pattern_marginal(p, odd, free),
                    None => free,
                };
                [hand, sigmoid(logits[1])]
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
    /// 分組特徵（分組架構才用）：每顆的特徵、所在的組與組內序號，以及每組的描述。
    pub g: Vec<[f32; G_COLS]>,
    pub gid: Vec<Option<usize>>,
    pub gpos: Vec<usize>,
    pub gdesc: Vec<[f32; GD_COLS]>,
}

/// 分組的種類（與 g1/build_data.py 的 group_type 對應）：0 同鍵、1 階梯、2 兩顆、3 其他、4 含 Touch。
fn group_type(notes: &[&Note]) -> usize {
    let spec: Vec<(u8, bool)> = notes
        .iter()
        .map(|n| {
            let touch = n.touch_area.as_deref().is_some_and(|a| !a.is_empty());
            (n.button, touch)
        })
        .collect();
    group_kind(&spec)
}

/// 同 [`group_type`]，輸入每顆的（鍵位, 是不是 Touch）。
fn group_kind(spec: &[(u8, bool)]) -> usize {
    if spec.iter().any(|&(_, touch)| touch) {
        return 4;
    }
    let b: Vec<i32> = spec.iter().map(|&(button, _)| button as i32).collect();
    let steps: Vec<i32> = b.windows(2).map(|w| (w[1] - w[0]).rem_euclid(8)).collect();
    if b.iter().all(|&x| x == b[0]) {
        0
    } else if b.len() >= 3 && (steps.iter().all(|&s| s == 1) || steps.iter().all(|&s| s == 7)) {
        1
    } else if b.len() == 2 {
        2
    } else {
        3
    }
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
        g: vec![[0.0; G_COLS]; notes.len()],
        gid: vec![None; notes.len()],
        gpos: vec![0; notes.len()],
        gdesc: vec![],
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
    // 分組特徵（與 g1/build_data.py 的 build 逐項對應）：自動分組依序編號；
    // 和前面的組重疊、或譜面上不到兩顆的組略過。
    let pos: HashMap<&str, usize> = notes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.key.as_str(), i))
        .collect();
    let mut prev_start: Option<f64> = None;
    for (members, rule, auto) in solver::auto_group_info(chart) {
        let idx: Vec<usize> = members
            .iter()
            .filter_map(|n| pos.get(n.key.as_str()).copied())
            .collect();
        if idx.len() < 2 || idx.iter().any(|&i| out.gid[i].is_some()) {
            continue;
        }
        let size = idx.len();
        let (t0, t1) = (
            members[0].time_seconds,
            members[members.len() - 1].time_seconds,
        );
        let gap = prev_start.map_or(5.0, |p| t0 - p);
        prev_start = Some(t0);
        let mut desc = [0f32; GD_COLS];
        desc[group_type(&members)] = 1.0;
        if members.iter().any(|n| n.kind == "slide") {
            desc[GROUP_TYPES] = 1.0;
        }
        if rule < 3 {
            desc[GROUP_TYPES + 1 + rule as usize] = 1.0;
        }
        desc[GROUP_TYPES + 4 + auto.filter(|a| *a < 3).map_or(3, usize::from)] = 1.0;
        let log_size = (size as f64).ln_1p() as f32;
        desc[GD_COLS - 3] = log_size;
        desc[GD_COLS - 2] = lg(t1 - t0) as f32;
        desc[GD_COLS - 1] = lg(gap) as f32;
        let g = out.gdesc.len();
        out.gdesc.push(desc);
        for (k, &i) in idx.iter().enumerate() {
            out.gid[i] = Some(g);
            out.gpos[i] = k;
            let row = &mut out.g[i];
            row[0] = 1.0;
            row[1..G_COLS - 3].copy_from_slice(&desc[..GD_COLS - 3]);
            row[G_COLS - 3] = (k as f64 / (size - 1) as f64) as f32;
            row[G_COLS - 2] = if k == 0 { 1.0 } else { 0.0 };
            row[G_COLS - 1] = log_size;
        }
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

/// 模型給的手（`p` 是用右手的機率，`v3` 是 V3 的手：1 右、0 左）：和 V3 不同時，
/// 把握（較大那一邊的機率）要到 `flip_at` 才改，不到就照 V3。
fn chosen_hand(p: f32, v3: i8, flip_at: f32) -> Hand {
    let model = if p > 0.5 { Hand::R } else { Hand::L };
    let v3 = if v3 == 1 { Hand::R } else { Hand::L };
    if model != v3 && p.max(1.0 - p) < flip_at {
        v3
    } else {
        model
    }
}

/// 整組的手：在符合規則的打法（`rule` 0 同手：全右／全左；1 交替：右起／左起；
/// 2 兩者皆可：四種都算）與 V3 原本的手之間，取模型機率（對數）總和最大的；
/// 沒有打法比 V3 好就照 V3。`p` 是每顆用右手的機率，`v3` 是 V3 的手（true 右）。
fn group_choice(p: &[f32], v3: &[bool], rule: u8) -> Vec<bool> {
    let n = p.len();
    let score = |hands: &[bool]| {
        p.iter()
            .zip(hands)
            .map(|(&p, &right)| {
                let p = (p as f64).clamp(1e-4, 1.0 - 1e-4);
                if right {
                    p.ln()
                } else {
                    (1.0 - p).ln()
                }
            })
            .sum::<f64>()
    };
    let mut candidates: Vec<Vec<bool>> = vec![];
    if rule != 1 {
        candidates.push(vec![true; n]);
        candidates.push(vec![false; n]);
    }
    if rule != 0 {
        candidates.push((0..n).map(|k| k % 2 == 0).collect());
        candidates.push((0..n).map(|k| k % 2 == 1).collect());
    }
    let base = score(v3);
    candidates
        .into_iter()
        .map(|c| (score(&c), c))
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .filter(|(s, _)| *s > base)
        .map(|(_, c)| c)
        .unwrap_or_else(|| v3.to_vec())
}

/// 分組架構：有規則的自動分組整組決定接觸手（取代組內逐顆的修正）。
/// 含 Touch 的組不整組套用，照逐顆修正（研究時在開發集上選定的設定）。
fn apply_whole_groups(
    chart: &Chart,
    input: &Features,
    probs: &[[f32; 2]],
    constraints: &mut [Constraint],
) {
    let pos: HashMap<&str, usize> = input
        .order
        .iter()
        .enumerate()
        .map(|(i, &ni)| (chart.notes[ni].id.as_str(), i))
        .collect();
    for (notes, rule) in solver::rule_groups(chart) {
        if notes.iter().any(|n| n.touch_area.is_some()) {
            continue;
        }
        let idx: Vec<usize> = notes
            .iter()
            .filter_map(|n| pos.get(n.id.as_str()).copied())
            .filter(|&i| input.v3_hand[i] >= 0)
            .collect();
        if idx.len() != notes.len() || idx.len() < 2 {
            continue;
        }
        let p: Vec<f32> = idx.iter().map(|&i| probs[i][0]).collect();
        let v3: Vec<bool> = idx.iter().map(|&i| input.v3_hand[i] == 1).collect();
        for (&i, right) in idx.iter().zip(group_choice(&p, &v3, rule)) {
            let id = &chart.notes[input.order[i]].id;
            if let Some(k) = constraints
                .iter_mut()
                .find(|k| !k.track && &k.note_id == id)
            {
                k.hand = if right { Hand::R } else { Hand::L };
                k.flipped = right != (input.v3_hand[i] == 1);
            }
        }
    }
}

/// 放寬次數上限：每次在走不下去的時刻附近拿掉一些限制。純模型每顆都照模型、和基座不同的
/// 地方多很多，24 次常不夠用而整首退回基座，所以給 96 次。
const MAX_RELAX: usize = 24;
const MAX_RELAX_PURE: usize = 96;

/// Temoto 求解：V3（或模型附帶的權重）→ 模型修正 → 照修正後的手重新求解。
/// 照修正走不下去時，在卡住的時刻附近逐步拿掉限制（先拿掉修正的，再拿掉全部）再試；
/// 仍不行就回傳基座的結果並加註。提醒文字裡的基座名稱照模型附帶的權重（`base_label`）。
pub fn solve(
    chart: &Chart,
    c: &SolverConfig,
    model: &TemotoModel,
) -> Result<Vec<Solution>, Diagnostic> {
    let base_name = model.base_label();
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
            let mut push = |track: bool, p: f32, v3: i8| {
                let h = chosen_hand(p, v3, model.flip_at);
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
        if model.grouped() {
            apply_whole_groups(chart, &input, &probs, &mut constraints);
        }
        let mut last_fail: Option<f64> = None;
        let mut strikes = 0;
        let max_relax = if model.uses_base {
            MAX_RELAX
        } else {
            MAX_RELAX_PURE
        };
        for _ in 0..=max_relax {
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
                            "Temoto-1：修改了 {a} 個手順；另有 {d} 個修改動作上做不到，沿用 {base_name}。"
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
                format!("Temoto-1 的修正在這份譜面走不下去，改用 {base_name} 的結果。"),
            );
        }
        Ok(base)
    };
    match model.tuning() {
        Some(t) => crate::scoring_v3::with_tuning(t, run),
        None => run(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 和 V3 相同時照模型；不同時把握到門檻才改。
    #[test]
    fn corrections_need_enough_confidence() {
        // V3 左手，模型 0.55 偏右：0.5 改，0.6 不改
        assert_eq!(chosen_hand(0.55, 0, 0.5), Hand::R);
        assert_eq!(chosen_hand(0.55, 0, 0.6), Hand::L);
        // 把握 0.7 時兩個門檻都改，左右對稱
        assert_eq!(chosen_hand(0.7, 0, 0.6), Hand::R);
        assert_eq!(chosen_hand(0.3, 1, 0.6), Hand::L);
        assert_eq!(chosen_hand(0.45, 1, 0.6), Hand::R);
        // 和 V3 一致時不看門檻
        assert_eq!(chosen_hand(0.55, 1, 0.9), Hand::R);
        assert_eq!(chosen_hand(0.45, 0, 0.9), Hand::L);
    }

    /// 模型檔標頭可以帶改動門檻（flipAt），沒寫為 0.5。
    #[test]
    fn flip_at_comes_from_header() {
        let bytes: &[u8] = include_bytes!("../models/temoto-1-ruri.temoto");
        assert_eq!(TemotoModel::load(bytes).unwrap().flip_at(), 0.5);
        let len = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        let mut header: serde_json::Value = serde_json::from_slice(&bytes[12..12 + len]).unwrap();
        header["flipAt"] = 0.6.into();
        let header = serde_json::to_vec(&header).unwrap();
        let mut edited = bytes[..8].to_vec();
        edited.extend((header.len() as u32).to_le_bytes());
        edited.extend(&header);
        edited.extend(&bytes[12 + len..]);
        assert_eq!(TemotoModel::load(&edited).unwrap().flip_at(), 0.6);
    }

    /// 標頭 `input` 為 "none" 的純模型不看基座的手：基座的手怎麼換，預測都一樣；
    /// 一般模型則會跟著變。
    #[test]
    fn pure_model_ignores_base_hands() {
        let bytes: &[u8] = include_bytes!("../models/temoto-1-ruri.temoto");
        let len = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        let mut header: serde_json::Value = serde_json::from_slice(&bytes[12..12 + len]).unwrap();
        header["input"] = "none".into();
        let header = serde_json::to_vec(&header).unwrap();
        let mut edited = bytes[..8].to_vec();
        edited.extend((header.len() as u32).to_le_bytes());
        edited.extend(&header);
        edited.extend(&bytes[12 + len..]);
        let pure = TemotoModel::load(&edited).unwrap();
        let based = TemotoModel::load(bytes).unwrap();
        let chart = crate::parse_chart("(150){8}1,2,3,4,5,6,7,8,E", 0.0)
            .unwrap()
            .chart;
        let mut input = features(&chart, None);
        let (pure_before, based_before) = (pure.predict(&input), based.predict(&input));
        for hand in input.v3_hand.iter_mut() {
            *hand = 1;
        }
        assert_eq!(pure.predict(&input), pure_before);
        assert_ne!(based.predict(&input), based_before);
    }

    /// 整組套用：只在符合規則的打法裡選，而且要比 V3 原本的手更符合模型才改。
    #[test]
    fn whole_group_follows_rule_and_beats_v3() {
        let (r, l) = (true, false);
        // 模型偏向交替（右起），規則允許交替就整組改成右左右
        let alt = [0.8, 0.2, 0.7];
        assert_eq!(group_choice(&alt, &[r, r, r], 1), vec![r, l, r]);
        assert_eq!(group_choice(&alt, &[r, r, r], 2), vec![r, l, r]);
        // 規則只允許同手時不能交替：全右（0.8·0.2·0.7）比全左好，但沒有比 V3 的全右好，照 V3
        assert_eq!(group_choice(&alt, &[r, r, r], 0), vec![r, r, r]);
        // V3 的手比任何打法都符合模型時照 V3（即使 V3 本身不符合規則）
        assert_eq!(group_choice(&alt, &[r, l, r], 0), vec![r, l, r]);
        // 模型偏左：同手規則整組改全左
        assert_eq!(
            group_choice(&[0.1, 0.3, 0.2, 0.4], &[r, l, r, l], 0),
            vec![l; 4]
        );
    }

    /// 打法邊際：固定打法時單顆有 PATTERN_EPS 的機率不同，自由打法用逐顆的機率。
    #[test]
    fn pattern_marginal_mixes_patterns() {
        let eps = PATTERN_EPS;
        assert!((pattern_marginal(&[1.0, 0.0, 0.0, 0.0, 0.0], false, 0.9) - eps).abs() < 1e-6);
        assert!(
            (pattern_marginal(&[0.0, 1.0, 0.0, 0.0, 0.0], true, 0.1) - (1.0 - eps)).abs() < 1e-6
        );
        // 左起交替：偶數顆左、奇數顆右
        assert!((pattern_marginal(&[0.0, 0.0, 1.0, 0.0, 0.0], false, 0.5) - eps).abs() < 1e-6);
        assert!(
            (pattern_marginal(&[0.0, 0.0, 1.0, 0.0, 0.0], true, 0.5) - (1.0 - eps)).abs() < 1e-6
        );
        assert!(
            (pattern_marginal(&[0.0, 0.0, 0.0, 1.0, 0.0], false, 0.5) - (1.0 - eps)).abs() < 1e-6
        );
        assert!((pattern_marginal(&[0.0, 0.0, 0.0, 0.0, 1.0], false, 0.3) - 0.3).abs() < 1e-6);
    }

    /// 分組種類：Touch 優先，其次同鍵、單向逐鍵（含跨 8→1）的階梯、兩顆、其他。
    #[test]
    fn group_types_match_training() {
        let (n, t) = (false, true);
        assert_eq!(group_kind(&[(3, n), (3, n), (3, n)]), 0);
        assert_eq!(group_kind(&[(7, n), (8, n), (1, n)]), 1);
        assert_eq!(group_kind(&[(2, n), (1, n), (8, n)]), 1);
        // 兩顆相鄰不算階梯
        assert_eq!(group_kind(&[(1, n), (2, n)]), 2);
        assert_eq!(group_kind(&[(1, n), (2, n), (4, n)]), 3);
        assert_eq!(group_kind(&[(1, n), (0, t)]), 4);
    }
}
