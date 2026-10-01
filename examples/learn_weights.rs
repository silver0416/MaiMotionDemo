//! 從真人標註學 V3 的線性權重（結構化學習，cutting-plane 結構化 SVM）。
//!
//! 每一輪：用目前的權重對每首訓練譜求「模型最佳解」與「照標註的最佳解」，並在同一條
//! 路線上用探測權重重算總成本，拆出每個權重對應的特徵值（成本 = 常數 + Σ 權重 × 特徵）。
//! 模型解收進每首譜的候選池；在候選池上算結構化 hinge
//!   Σ 譜 max(0, max_候選 [γ·不吻合數 − (成本候選 − 成本標註)])
//! 的梯度，沿它做乘法更新（權重維持非負、為 0 的維持 0），步長用實際的訓練吻合率挑。
//! 非線性的參數（窗口、遞減速度、手感設定等）固定為起始值。
//!
//! cargo run --release --example learn_weights -- 整包.json --train 0,2,4 [--eval 1,3]
//!   [--start 權重.json] [--config 設定.json] [--iters 8] [--margin 0.5]
//!   [--seed splitUpper=0.05,...] [--freeze travel,...] [--refweight 10] [--out 學到的權重.json]
use mai_motion_core::annotation::evaluate_annotation_probed;
use mai_motion_core::scoring_v3::{with_tuning, TuningV3};
use mai_motion_core::*;
use serde_json::{json, Value};

/// 參考手順（speed_first_full）在整包裡的索引；不吻合一次算 refweight 次。
const REF_INDEX: usize = 9;

/// 學的線性權重。衍生項（滑行速度、階梯速度、和弦換手）是兩個欄位的乘積，
/// 學的是乘積，寫回時再除開。
const NAMES: [&str; 21] = [
    "travel",
    "speedStrain",
    "glideSpeed*",
    "stairSpeed*",
    "swappedPosture",
    "contactCross",
    "homeEntry",
    "homeExposure",
    "reversal",
    "jack",
    "workload",
    "ownershipSwitch",
    "chordSwitch*",
    "anchorHold",
    "starSwitch",
    "stairBreak",
    "strokeBreak",
    "shapeMix",
    "phraseTemplate",
    "phraseOneHand",
    "splitUpper",
];
const K: usize = NAMES.len();

/// 把學到的線性值寫回 TuningV3（非線性欄位沿用 base）。
fn to_tuning(base: &TuningV3, u: &[f64; K]) -> TuningV3 {
    let mut t = base.clone();
    let st = u[1].max(1e-4);
    let os = u[11].max(1e-4);
    t.travel = u[0];
    t.speed_strain = u[1];
    t.glide_speed = u[2] / st;
    t.stair_speed = u[3] / st;
    t.swapped_posture = u[4];
    t.contact_cross = u[5];
    t.home_entry = u[6];
    t.home_exposure = u[7];
    t.reversal = u[8];
    t.jack = u[9];
    t.workload = u[10];
    t.ownership_switch = u[11];
    t.chord_switch_discount = u[12] / os;
    t.anchor_hold = u[13];
    t.star_switch = u[14];
    t.stair_break = u[15];
    t.stroke_break = u[16];
    t.shape_mix = u[17];
    t.phrase_template = u[18];
    t.phrase_one_hand = u[19];
    t.split_upper = u[20];
    t
}

fn from_tuning(t: &TuningV3) -> [f64; K] {
    [
        t.travel,
        t.speed_strain,
        t.speed_strain * t.glide_speed,
        t.speed_strain * t.stair_speed,
        t.swapped_posture,
        t.contact_cross,
        t.home_entry,
        t.home_exposure,
        t.reversal,
        t.jack,
        t.workload,
        t.ownership_switch,
        t.ownership_switch * t.chord_switch_discount,
        t.anchor_hold,
        t.star_switch,
        t.stair_break,
        t.stroke_break,
        t.shape_mix,
        t.phrase_template,
        t.phrase_one_hand,
        t.split_upper,
    ]
}

/// 探測權重：0 號全部線性權重歸零（常數項），其餘各開一項；衍生項疊在基底項上，
/// 特徵 = 該探測 − 參考探測。
fn probes(base: &TuningV3) -> (Vec<&'static TuningV3>, [(usize, usize); K]) {
    let mut zero = base.clone();
    let off = [0.0; K];
    zero = TuningV3 {
        glide_speed: 0.0,
        stair_speed: 0.0,
        chord_switch_discount: 0.0,
        ..to_tuning(&zero, &off)
    };
    let mut list = vec![zero.clone()];
    let mut pairs = [(0, 0); K];
    for k in 0..K {
        let mut t = zero.clone();
        let reference = match k {
            2 => {
                t.speed_strain = 1.0;
                t.glide_speed = 1.0;
                2 // speedStrain 的探測（索引 1 + 1）
            }
            3 => {
                t.speed_strain = 1.0;
                t.stair_speed = 1.0;
                2
            }
            12 => {
                t.ownership_switch = 1.0;
                t.chord_switch_discount = 1.0;
                12 // ownershipSwitch 的探測（索引 11 + 1）
            }
            _ => {
                let mut u = [0.0; K];
                u[k] = 1.0;
                t = TuningV3 {
                    glide_speed: 0.0,
                    stair_speed: 0.0,
                    chord_switch_discount: 0.0,
                    ..to_tuning(&zero, &u)
                };
                0
            }
        };
        list.push(t);
        pairs[k] = (k + 1, reference);
    }
    (
        list.into_iter().map(|t| &*Box::leak(Box::new(t))).collect(),
        pairs,
    )
}

#[derive(Clone)]
struct Path {
    constant: f64,
    features: [f64; K],
    /// 加權後的不吻合數。
    loss: f64,
}
impl Path {
    fn from_totals(totals: &[f64], pairs: &[(usize, usize); K], loss: f64) -> Option<Self> {
        if totals.len() != K + 1 {
            return None;
        }
        let mut features = [0.0; K];
        for (k, (probe, reference)) in pairs.iter().enumerate() {
            features[k] = totals[*probe] - totals[*reference];
        }
        Some(Self {
            constant: totals[0],
            features,
            loss,
        })
    }
    fn cost(&self, u: &[f64; K]) -> f64 {
        self.constant + self.features.iter().zip(u).map(|(f, w)| f * w).sum::<f64>()
    }
}

struct Outcome {
    index: usize,
    compared: usize,
    agreed: usize,
    model: Option<Path>,
    human: Option<Path>,
    /// 模型解的實際總成本與線性重組的差（檢查特徵拆解是否正確）。
    check: f64,
}

fn run(
    items: &[(usize, HandAnnotation)],
    config: &SolverConfig,
    tuning: &'static TuningV3,
    probes: &[&'static TuningV3],
    pairs: &[(usize, usize); K],
    refweight: f64,
) -> Vec<Outcome> {
    let u = from_tuning(tuning);
    std::thread::scope(|scope| {
        let handles: Vec<_> = items
            .iter()
            .map(|(index, annotation)| {
                scope.spawn(move || {
                    let request = EvaluateRequest {
                        request_id: "learn".into(),
                        source: annotation.chart.source.clone(),
                        first_seconds: annotation.chart.first_seconds,
                        solver_config: config.clone(),
                        annotation: annotation.clone(),
                    };
                    let (response, model, human) =
                        with_tuning(tuning, || evaluate_annotation_probed(request, probes));
                    let weight = if *index == REF_INDEX { refweight } else { 1.0 };
                    let loss = weight * (response.compared - response.agreed) as f64;
                    let model = Path::from_totals(&model, pairs, loss);
                    let human = Path::from_totals(&human, pairs, 0.0);
                    let check = match (&model, &response.model) {
                        (Some(path), Some(solution)) => (path.cost(&u) - solution.cost).abs(),
                        _ => 0.0,
                    };
                    Outcome {
                        index: *index,
                        compared: response.compared,
                        agreed: response.agreed,
                        model,
                        human,
                        check,
                    }
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    })
}

/// 候選池上結構化 hinge 對權重的梯度：每首譜取違反最大的候選，
/// 梯度 = Σ (標註解特徵 − 該候選特徵) × 譜的權重。
fn hinge_gradient(pools: &[(Vec<Path>, Path)], u: &[f64; K], margin: f64) -> ([f64; K], f64) {
    let mut grad = [0.0; K];
    let mut objective = 0.0;
    for (pool, human) in pools {
        let human_cost = human.cost(u);
        let worst = pool
            .iter()
            .map(|y| (margin * y.loss - (y.cost(u) - human_cost), y))
            .max_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((violation, y)) = worst.filter(|(v, _)| *v > 0.0) {
            objective += violation;
            for (g, (h, f)) in grad.iter_mut().zip(human.features.iter().zip(&y.features)) {
                *g += h - f;
            }
        }
    }
    (grad, objective)
}

/// 乘法更新的方向（對數尺度）：最大的一項為 ±1。權重為 0 的項維持 0。
fn direction(grad: &[f64; K], u: &[f64; K], frozen: &[bool; K]) -> [f64; K] {
    let raw: [f64; K] = std::array::from_fn(|k| if frozen[k] { 0.0 } else { -grad[k] * u[k] });
    let top = raw.iter().map(|x| x.abs()).fold(0.0, f64::max).max(1e-300);
    std::array::from_fn(|k| raw[k] / top)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let bundle_path = args
        .next()
        .expect("用法：learn_weights <整包.json> --train 0,2 ...");
    let mut config = SolverConfig::v3();
    let mut base = TuningV3::default();
    let mut train: Vec<usize> = vec![];
    let mut eval: Vec<usize> = vec![];
    let (mut iters, mut margin, mut refweight) = (8usize, 0.5, 10.0);
    let mut seeds: Vec<(String, f64)> = vec![];
    let mut out: Option<String> = None;
    let mut freeze: Vec<String> = vec![];
    let list = |s: String| -> Vec<usize> {
        s.split(',')
            .filter(|x| !x.is_empty())
            .map(|x| x.parse().unwrap())
            .collect()
    };
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--config" => {
                let text = std::fs::read_to_string(args.next().unwrap()).expect("讀不到設定");
                config = serde_json::from_str(&text).expect("設定格式錯誤");
            }
            "--start" => {
                let text = std::fs::read_to_string(args.next().unwrap()).expect("讀不到權重");
                base = serde_json::from_str(&text).expect("權重格式錯誤");
            }
            "--templates" => {
                let text = std::fs::read_to_string(args.next().unwrap()).expect("讀不到樣板");
                set_templates(serde_json::from_str(&text).expect("樣板格式錯誤"));
            }
            "--train" => train = list(args.next().unwrap()),
            "--eval" => eval = list(args.next().unwrap()),
            "--iters" => iters = args.next().unwrap().parse().unwrap(),
            "--margin" => margin = args.next().unwrap().parse().unwrap(),
            "--seed" => {
                for pair in args.next().unwrap().split(',') {
                    let (name, value) = pair.split_once('=').expect("--seed 名稱=值");
                    seeds.push((name.into(), value.parse().unwrap()));
                }
            }
            "--refweight" => refweight = args.next().unwrap().parse().unwrap(),
            "--freeze" => freeze = args.next().unwrap().split(',').map(String::from).collect(),
            "--out" => out = args.next(),
            other => panic!("不認得的參數 {other}"),
        }
    }
    let frozen: [bool; K] = std::array::from_fn(|k| freeze.iter().any(|f| f == NAMES[k]));
    let bundle: Value =
        serde_json::from_str(&std::fs::read_to_string(&bundle_path).expect("讀不到整包檔"))
            .expect("整包檔格式錯誤");
    let load = |indices: &[usize]| -> Vec<(usize, HandAnnotation)> {
        indices
            .iter()
            .map(|&i| {
                let annotation = bundle["items"][i]["annotation"].clone();
                (i, serde_json::from_value(annotation).expect("標註格式錯誤"))
            })
            .collect()
    };
    let train_items = load(&train);
    let eval_items = load(&eval);
    let (probe_list, pairs) = probes(&base);
    let mut u = from_tuning(&base);
    for (name, value) in &seeds {
        let k = NAMES.iter().position(|n| n == name).expect("沒有這個權重");
        u[k] = *value;
    }
    let mut pools: Vec<Vec<Path>> = vec![vec![]; train_items.len()];
    let summary = |outcomes: &[Outcome]| {
        let main: Vec<_> = outcomes.iter().filter(|o| o.index != REF_INDEX).collect();
        let reference = outcomes.iter().find(|o| o.index == REF_INDEX);
        let old: Vec<_> = main.iter().filter(|o| o.index < REF_INDEX).collect();
        let new: Vec<_> = main.iter().filter(|o| o.index > REF_INDEX).collect();
        let sum = |xs: &[&&Outcome]| {
            (
                xs.iter().map(|o| o.agreed).sum::<usize>(),
                xs.iter().map(|o| o.compared).sum::<usize>(),
            )
        };
        let all: Vec<&&Outcome> = main.iter().collect();
        let (a, c) = sum(&all);
        let (oa, oc) = sum(&old.to_vec());
        let (na, nc) = sum(&new.to_vec());
        format!(
            "{a}/{c} {:.2}% 舊 {oa}/{oc} 新 {na}/{nc} ref {}",
            100.0 * a as f64 / c.max(1) as f64,
            reference.map_or("-".into(), |r| format!("{}/{}", r.agreed, r.compared))
        )
    };
    let mut history = vec![];
    let steps = [0.05, 0.1, 0.2, 0.4];
    let mut shrink = 1.0;
    for round in 0..=iters {
        let tuning: &'static TuningV3 = Box::leak(Box::new(to_tuning(&base, &u)));
        let outcomes = run(
            &train_items,
            &config,
            tuning,
            &probe_list,
            &pairs,
            refweight,
        );
        let check = outcomes.iter().map(|o| o.check).fold(0.0, f64::max);
        let held = if eval_items.is_empty() {
            String::new()
        } else {
            let e = run(&eval_items, &config, tuning, &probe_list, &pairs, refweight);
            format!("  驗證 {}", summary(&e))
        };
        let line = format!(
            "round {round} 訓練 {}{held}  (線性重組誤差 {check:.2e})",
            summary(&outcomes)
        );
        eprintln!("{line}");
        history.push(json!({"round": round, "line": line, "tuning": tuning}));
        if round == iters {
            break;
        }
        if std::env::var_os("LEARN_BREAKDOWN").is_some() {
            // 標註解比模型解多付的成本，拆到每一項：Σ 權重 × (標註特徵 − 模型特徵)。
            let pairs: Vec<(&Path, &Path)> = outcomes
                .iter()
                .filter_map(|o| Some((o.model.as_ref()?, o.human.as_ref()?)))
                .collect();
            let constant: f64 = pairs.iter().map(|(m, h)| h.constant - m.constant).sum();
            eprintln!("  常數項（壓縮、換手）差 {constant:.2}");
            for k in 0..K {
                let diffs: Vec<f64> = pairs
                    .iter()
                    .map(|(m, h)| u[k] * (h.features[k] - m.features[k]))
                    .collect();
                let raw: f64 = pairs
                    .iter()
                    .map(|(m, h)| h.features[k] - m.features[k])
                    .sum();
                let up = diffs.iter().filter(|d| **d > 1e-9).count();
                let down = diffs.iter().filter(|d| **d < -1e-9).count();
                eprintln!(
                    "  {:<16} 標註多付 {:>9.2}  (原始特徵差 {:>10.3}；{} 首較貴、{} 首較便宜)",
                    NAMES[k],
                    diffs.iter().sum::<f64>(),
                    raw,
                    up,
                    down
                );
            }
        }
        if std::env::var_os("LEARN_DEBUG").is_some() {
            for o in &outcomes {
                if let (Some(m), Some(h)) = (&o.model, &o.human) {
                    eprintln!(
                        "  #{} 不吻合 {:.0} 模型 {:.3} 標註 {:.3} 差 {:.3}",
                        o.index,
                        m.loss,
                        m.cost(&u),
                        h.cost(&u),
                        h.cost(&u) - m.cost(&u)
                    );
                }
            }
        }
        let mut humans = vec![];
        for (slot, outcome) in outcomes.iter().enumerate() {
            if let Some(model) = &outcome.model {
                let fresh = !pools[slot].iter().any(|p| {
                    p.features
                        .iter()
                        .zip(&model.features)
                        .all(|(a, b)| (a - b).abs() < 1e-9)
                });
                if fresh {
                    pools[slot].push(model.clone());
                }
            }
            humans.push(outcome.human.clone());
        }
        let data: Vec<(Vec<Path>, Path)> = pools
            .iter()
            .zip(humans)
            .filter_map(|(pool, human)| human.map(|h| (pool.clone(), h)))
            .collect();
        let (grad, violation) = hinge_gradient(&data, &u, margin);
        let dir = direction(&grad, &u, &frozen);
        // 步長用實際的訓練吻合（+ refweight × 參考手順）挑；試過的模型解也收進候選池。
        let score = |o: &[Outcome]| {
            o.iter()
                .map(|x| x.agreed as f64 * if x.index == REF_INDEX { refweight } else { 1.0 })
                .sum::<f64>()
        };
        let current = score(&outcomes);
        let mut best: Option<(f64, [f64; K])> = None;
        for eta in steps.iter().map(|s| s * shrink) {
            let trial: [f64; K] = std::array::from_fn(|k| u[k] * (eta * dir[k]).exp());
            let tuning: &'static TuningV3 = Box::leak(Box::new(to_tuning(&base, &trial)));
            let o = run(
                &train_items,
                &config,
                tuning,
                &probe_list,
                &pairs,
                refweight,
            );
            for (slot, outcome) in o.iter().enumerate() {
                if let Some(model) = &outcome.model {
                    if !pools[slot].iter().any(|p| {
                        p.features
                            .iter()
                            .zip(&model.features)
                            .all(|(a, b)| (a - b).abs() < 1e-9)
                    }) {
                        pools[slot].push(model.clone());
                    }
                }
            }
            let value = score(&o);
            eprintln!("  步長 {eta:.3} → {} ({:+})", summary(&o), value - current);
            if best.as_ref().is_none_or(|(b, _)| value > *b) {
                best = Some((value, trial));
            }
        }
        match best {
            Some((value, trial)) if value > current => u = trial,
            _ => {
                shrink *= 0.25;
                eprintln!("  沒有進步，步長縮小為 {shrink}");
                if shrink < 0.01 {
                    break;
                }
            }
        }
        let shown: Vec<String> = (0..K)
            .filter(|&k| u[k] != 0.0)
            .map(|k| format!("{}={:.4}", NAMES[k], u[k]))
            .collect();
        eprintln!("  hinge {violation:.1}  權重 {}", shown.join(" "));
    }
    if let Some(path) = out {
        std::fs::write(
            path,
            serde_json::to_string_pretty(
                &json!({"tuning": to_tuning(&base, &u), "history": history}),
            )
            .unwrap(),
        )
        .expect("寫不出結果");
    }
}
