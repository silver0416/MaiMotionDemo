//! 真人手順標註：音符穩定鍵、標註檔格式與「照標註求解」的比對評估。
//!
//! 標註檔是 JSON（format = maimotion-hand-annotation），自帶原譜，前端負責編輯與合併，
//! 核心只讀取。音符以穩定鍵對應，不依賴 `n123` 這類會隨解析器改版位移的 id。

use crate::solver::{self, HandRules};
use crate::{Chart, Diagnostic, Hand, Solution, SolutionScore, SolverConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const ANNOTATION_FORMAT: &str = "maimotion-hand-annotation";
pub const ANNOTATION_VERSION: u32 = 1;

/// 每顆音符的穩定鍵：`時間|種類|位置`，時間取 3 位小數秒；同鍵重複時依譜面順序加 `#2`、`#3`。
/// 位置：外圈鍵為鍵號、Touch 為感應區（`B3`、`C`）、Slide 為 `起點形狀終點`（`1-5`、`1pp5`）。
pub fn assign_keys(chart: &mut Chart) {
    let mut seen: HashMap<String, usize> = HashMap::new();
    for i in 0..chart.notes.len() {
        let note = &chart.notes[i];
        let place = match (&note.touch_area, &note.path_id) {
            (Some(area), _) if area == "C" => "C".to_string(),
            (Some(area), _) => format!("{area}{}", note.button),
            (None, Some(id)) => chart
                .paths
                .iter()
                .find(|p| &p.id == id)
                .map(|p| format!("{}{}{}", p.start_button, p.shape, p.end_button))
                .unwrap_or_else(|| note.button.to_string()),
            (None, None) => note.button.to_string(),
        };
        let base = format!("{:.3}|{}|{}", note.time_seconds, note.kind, place);
        let count = seen.entry(base.clone()).or_insert(0);
        *count += 1;
        chart.notes[i].key = if *count == 1 {
            base
        } else {
            format!("{base}#{count}")
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackHand {
    L,
    R,
    /// 兩手一起（WiFi 2+1）。
    LR,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Confidence {
    /// 確定：拿來比對與限制求解。
    #[default]
    Sure,
    /// 不確定：只列出，不限制求解、不計入吻合率。
    Unsure,
    /// 兩手皆可：不限制求解；比對時不論模型用哪隻手都算吻合。
    Either,
}

/// 標註的一步：接觸（Tap／Hold／Touch、Slide 起點）或 Slide 滑行。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StepPart {
    Hand,
    Track,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoverMark {
    /// 換手的時刻（秒）。
    pub at: f64,
    pub to: Hand,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteAnnotation {
    pub key: String,
    /// 接觸的手：Tap／Hold／Touch，或 Slide 的起點觸碰。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hand: Option<Hand>,
    /// Slide 開始滑行時的手。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track: Option<TrackHand>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub handovers: Vec<HandoverMark>,
    /// 接觸那一步（Tap／Hold／Touch、Slide 起點；沒有起點的 Slide 則是滑行）的信心。
    #[serde(default)]
    pub confidence: Confidence,
    /// 有起點的 Slide 滑行那一步的信心；省略時沿用 confidence（舊標註檔整顆共用一個信心）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track_confidence: Option<Confidence>,
    /// 由模型或影片預填、還沒有人確認；不當作真人資料。只要有一步是預填就是 true。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub prefilled: bool,
    /// 只有這一步是預填，另一步是真人標的；省略時有手的步驟都是預填（舊標註檔的意義不變）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefilled_part: Option<StepPart>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub memo: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub by: String,
}

impl NoteAnnotation {
    /// 滑行那一步的信心：起點與滑行可以分開設定（例如起點確定、滑行兩手皆可）。
    pub fn slide_confidence(&self) -> Confidence {
        self.track_confidence.unwrap_or(self.confidence)
    }

    /// 還沒確認的預填蓋到哪幾步：（接觸, 滑行）。只看有手的步驟。
    pub fn prefilled_steps(&self) -> (bool, bool) {
        if !self.prefilled {
            return (false, false);
        }
        (
            self.hand.is_some() && self.prefilled_part != Some(StepPart::Track),
            self.track.is_some() && self.prefilled_part != Some(StepPart::Hand),
        )
    }

    /// 只留真人確認過的步驟：拿掉預填的手（信心與備註照舊）。沒有剩下任何手時回傳 None。
    pub fn confirmed(&self) -> Option<NoteAnnotation> {
        let (hand, track) = self.prefilled_steps();
        let mut out = self.clone();
        if hand {
            out.hand = None;
        }
        if track {
            out.track = None;
            out.handovers.clear();
        }
        out.prefilled = false;
        out.prefilled_part = None;
        (out.hand.is_some() || out.track.is_some()).then_some(out)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RangeMemo {
    pub from: f64,
    pub to: f64,
    pub memo: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub by: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotatedChart {
    /// 原文 SHA-256（十六進位）。
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub first_seconds: f64,
    #[serde(default)]
    pub note_count: usize,
    pub source: String,
}

/// 標註檔（`*.maimotion-hands.json`）。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandAnnotation {
    pub format: String,
    pub version: u32,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub annotators: Vec<String>,
    #[serde(default)]
    pub updated_at: String,
    pub chart: AnnotatedChart,
    #[serde(default)]
    pub memo: String,
    #[serde(default)]
    pub notes: Vec<NoteAnnotation>,
    #[serde(default)]
    pub ranges: Vec<RangeMemo>,
    /// 對照用的真人影片（網址、標題、同步偏移）；核心不使用，原樣保留。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluateRequest {
    pub request_id: String,
    /// 要比對的原文；通常與標註檔內的原譜相同。
    pub source: String,
    #[serde(default)]
    pub first_seconds: f64,
    #[serde(default)]
    pub solver_config: SolverConfig,
    pub annotation: HandAnnotation,
    /// 前端已有的模型最佳解（同一份原文、同一組設定分析出來的第一名）。有的話直接拿來比，
    /// 不再重算；呼叫端要確保它對應這份原文與設定。只用到指派、換手與分數。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_solution: Option<Solution>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluatedSolution {
    /// V2／V3 為分數 total，Legacy 為總成本；越低越好。
    pub cost: f64,
    pub score: Option<SolutionScore>,
    pub scoring_model: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Divergence {
    pub note_id: String,
    pub key: String,
    pub time_seconds: f64,
    /// hand（接觸）或 track（Slide 滑行開始）。
    pub part: String,
    pub human: TrackHand,
    pub model: TrackHand,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluateResponse {
    pub request_id: String,
    /// ok：兩邊都算完；human_infeasible：模型照標註有走不下去的地方（human 有值時，是擱著
    /// diagnostics 裡 annotation_skipped 那幾處標記後解完的）；invalid 等：原譜或設定有誤。
    /// 模型本身走不下去的地方兩邊都略過、不比對（diagnostics 的 model_skipped），不影響狀態。
    pub status: String,
    pub diagnostics: Vec<Diagnostic>,
    /// 有手部資訊、非預填的標註數。
    pub labeled: usize,
    /// 其中對得到音符的數量。
    pub matched: usize,
    pub unmatched_keys: Vec<String>,
    /// 參與比對（確定與皆可）的項目數與吻合數；皆可一律算吻合。
    pub compared: usize,
    pub agreed: usize,
    pub divergences: Vec<Divergence>,
    pub model: Option<EvaluatedSolution>,
    pub human: Option<EvaluatedSolution>,
    /// 照標註求得的完整方案，可在盤面上檢視。
    pub human_solution: Option<Solution>,
}

fn evaluated(solution: &Solution) -> EvaluatedSolution {
    let cost = match &solution.score {
        Some(SolutionScore::V3(s)) => s.total(),
        Some(SolutionScore::V2(s)) => s.ranking_value(),
        None => solution.total_cost,
    };
    EvaluatedSolution {
        cost,
        score: solution.score.clone(),
        scoring_model: solution.scoring_model.clone(),
    }
}

/// 模型在某顆音符上的手：接觸取第一次接觸；Slide 滑行取開始時的手，同時兩手為 LR。
fn model_hands(solution: &Solution, note_id: &str) -> (Option<Hand>, Option<TrackHand>) {
    let list: Vec<_> = solution
        .assignments
        .iter()
        .filter(|a| a.note_id == note_id)
        .collect();
    let contact = list
        .iter()
        .filter(|a| a.part != "slide")
        .min_by(|a, b| a.start_seconds.total_cmp(&b.start_seconds))
        .map(|a| a.hand);
    let slides: Vec<_> = list.iter().filter(|a| a.part == "slide").collect();
    let track = slides
        .iter()
        .min_by(|a, b| a.start_seconds.total_cmp(&b.start_seconds))
        .map(|first| {
            let both = slides.iter().any(|a| {
                a.hand != first.hand
                    && a.start_seconds < first.end_seconds
                    && first.start_seconds < a.end_seconds
                    && !solution.handovers.iter().any(|h| {
                        h.note_id == note_id && (h.start_seconds - a.start_seconds).abs() < 1e-9
                    })
            });
            if both {
                TrackHand::LR
            } else if first.hand == Hand::L {
                TrackHand::L
            } else {
                TrackHand::R
            }
        });
    (contact, track)
}

fn as_track(hand: Hand) -> TrackHand {
    if hand == Hand::L {
        TrackHand::L
    } else {
        TrackHand::R
    }
}

/// 把確定的標註轉成求解限制（音符 id）；接觸與滑行各看自己那一步的信心。
/// 滑行標的是開始滑的手：沒標換手時間的先當成整條同一隻手，走不下去時才由模型決定要不要
/// 中途換手（見 solver::solve_constrained）。相鄰兩鍵同時標成同一隻手的，允許那隻手張開一起按下。
fn rules(chart: &Chart, marks: &[(usize, &NoteAnnotation)], c: &SolverConfig) -> HandRules {
    let mut rules = HandRules {
        pair_press: true,
        ..HandRules::default()
    };
    for (index, mark) in marks {
        let note = &chart.notes[*index];
        if let Some(hand) = mark.hand {
            if mark.confidence == Confidence::Sure && (note.has_head || note.path_id.is_none()) {
                rules.contact.insert(note.id.clone(), hand);
            }
        }
        if note.path_id.is_none() || mark.slide_confidence() != Confidence::Sure {
            continue;
        }
        match mark.track {
            Some(TrackHand::LR) => {
                rules.track.insert(note.id.clone(), vec![]);
            }
            Some(first) => {
                let mut schedule = vec![(
                    f64::NEG_INFINITY,
                    if first == TrackHand::L {
                        Hand::L
                    } else {
                        Hand::R
                    },
                )];
                let mut handovers = mark.handovers.clone();
                handovers.sort_by(|a, b| a.at.total_cmp(&b.at));
                schedule.extend(handovers.iter().map(|h| (h.at, h.to)));
                rules.track.insert(note.id.clone(), schedule);
                if let (true, Some(start)) = (handovers.is_empty(), note.motion_start) {
                    rules
                        .track_start
                        .insert(note.id.clone(), start + c.slide_pickup_seconds);
                }
            }
            None => {}
        }
    }
    rules
}

/// 比對真人標註與模型：分別求出模型最佳解與「照標註打」的最佳解，列出不同的音符與成本差距。
/// 沒有現成的模型解時，模型與照標註兩個求解同時跑。照標註走不下去的地方先擱著那幾顆的
/// 標記、照模型的手接下去（solve_constrained_skipping），每一處列成 annotation_skipped。
/// 模型本身走不下去時用部分解：模型略過的音符照標註時也略過、不比對，每一處列成 model_skipped。
pub fn evaluate_annotation(request: EvaluateRequest) -> EvaluateResponse {
    // 選了 V3 變體時，模型解與照標註解都用它的權重，成本差距才在同一把尺上
    let variant = request.solver_config.v3_variant.clone();
    crate::scoring_v3::with_variant(variant.as_deref(), || {
        evaluate_inner(
            request,
            true,
            |chart, config| solve_model(chart, config, true),
            solver::solve_constrained_skipping,
        )
    })
}

/// 模型解；partial 時模型本身走不下去也略過做不到的音符解完（見 solver::solve_partial）。
fn solve_model(
    chart: &Chart,
    config: &SolverConfig,
    partial: bool,
) -> Result<Vec<Solution>, Diagnostic> {
    match config
        .ml_model
        .as_deref()
        .and_then(crate::temoto::TemotoModel::builtin)
    {
        // 比對對象跟著介面選的模型：選了 Temoto 就和 Temoto 修正後的結果比
        Some(model) if partial => crate::temoto::solve_partial(chart, config, model),
        Some(model) => crate::temoto::solve(chart, config, model),
        None if partial => solver::solve_partial(chart, config),
        None => solver::solve(chart, config),
    }
}

/// 學權重用：同 [`evaluate_annotation`]，另外回傳模型解與照標註解在每組探測權重下的
/// V3 總成本（同一條路線重算）。沒有該解時為空。
#[doc(hidden)]
pub fn evaluate_annotation_probed(
    request: EvaluateRequest,
    probes: &[&'static crate::scoring_v3::TuningV3],
) -> (EvaluateResponse, Vec<f64>, Vec<f64>) {
    let (mut model_totals, mut human_totals) = (vec![], vec![]);
    // 探測權重放在執行緒區域變數，所以兩個求解依序在這個執行緒上跑。
    let response = evaluate_inner(
        request,
        false,
        // 學權重只用完整的解，模型走不下去就是沒有解。
        |chart, config| {
            let (result, totals) =
                solver::with_probes(probes, || solve_model(chart, config, false));
            if result.is_ok() {
                model_totals = totals;
            }
            result
        },
        // 學權重只用完全照標註的解，不擱置標記。
        |chart, config, rules| {
            let (result, totals) =
                solver::with_probes(probes, || solver::solve_constrained(chart, config, rules));
            if result.is_ok() {
                human_totals = totals;
            }
            (result, vec![])
        },
    );
    (response, model_totals, human_totals)
}

type Solved = Result<Vec<Solution>, Diagnostic>;

fn evaluate_inner(
    mut request: EvaluateRequest,
    parallel: bool,
    solve_model: impl FnOnce(&Chart, &SolverConfig) -> Solved + Send,
    // 照標註的解，以及擱著標記的每一處（annotation_skipped）。
    mut solve_human: impl FnMut(&Chart, &SolverConfig, &HandRules) -> (Solved, Vec<Diagnostic>),
) -> EvaluateResponse {
    let supplied = request.model_solution.take();
    let mut response = EvaluateResponse {
        request_id: request.request_id.clone(),
        status: "invalid".into(),
        diagnostics: vec![],
        labeled: 0,
        matched: 0,
        unmatched_keys: vec![],
        compared: 0,
        agreed: 0,
        divergences: vec![],
        model: None,
        human: None,
        human_solution: None,
    };
    let annotation = &request.annotation;
    if annotation.format != ANNOTATION_FORMAT || annotation.version != ANNOTATION_VERSION {
        response.diagnostics.push(Diagnostic::plain(
            "invalid_annotation",
            format!("不是可讀的標註檔：需要 {ANNOTATION_FORMAT} 第 {ANNOTATION_VERSION} 版"),
        ));
        return response;
    }
    if let Err(message) = request.solver_config.validate() {
        response
            .diagnostics
            .push(Diagnostic::plain("invalid_config", message));
        return response;
    }
    let chart = match crate::parse_chart(&request.source, request.first_seconds) {
        Ok(parsed) => parsed.chart,
        Err(diagnostic) => {
            response.status = diagnostic.code.clone();
            response.diagnostics.push(diagnostic);
            return response;
        }
    };
    let by_key: HashMap<&str, usize> = chart
        .notes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.key.as_str(), i))
        .collect();
    // 預填的步驟不是真人資料：同一顆的真人步驟照樣比對，預填的那一步拿掉。
    let confirmed: Vec<NoteAnnotation> = annotation
        .notes
        .iter()
        .filter_map(NoteAnnotation::confirmed)
        .collect();
    let mut marks: Vec<(usize, &NoteAnnotation)> = vec![];
    for mark in &confirmed {
        response.labeled += 1;
        match by_key.get(mark.key.as_str()) {
            Some(index) => marks.push((*index, mark)),
            None => response.unmatched_keys.push(mark.key.clone()),
        }
    }
    response.matched = marks.len();

    let config = &request.solver_config;
    let mut rules = rules(&chart, &marks, config);
    // 模型（部分解）略過的音符，照標註求解時一樣略過，兩邊才是同一份譜面。
    let skipped_by = |model: &Solved| {
        model
            .as_ref()
            .ok()
            .and_then(|solutions| solutions.first())
            .map_or(vec![], |best| best.skipped.clone())
    };
    let (model, human) = match supplied {
        Some(solution) => {
            rules.skip = solution.skipped.clone();
            (Ok(vec![solution]), solve_human(&chart, config, &rules))
        }
        None if parallel => {
            // 另一個執行緒要帶上這次工作的取消旗標與評分權重（兩者都放在執行緒區域變數）。
            let cancel = solver::current_cancel();
            let tuning = crate::scoring_v3::current_override();
            std::thread::scope(|scope| {
                let chart = &chart;
                let model = scope.spawn(move || {
                    let run = || match tuning {
                        Some(tuning) => {
                            crate::scoring_v3::with_tuning(tuning, || solve_model(chart, config))
                        }
                        None => solve_model(chart, config),
                    };
                    match cancel {
                        Some(flag) => solver::with_cancel(flag, run),
                        None => run(),
                    }
                });
                let human = solve_human(chart, config, &rules);
                let model = model
                    .join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
                (model, human)
            })
        }
        None => {
            let model = solve_model(&chart, config);
            rules.skip = skipped_by(&model);
            let human = solve_human(&chart, config, &rules);
            (model, human)
        }
    };
    // 同時跑的照標註求解不知道模型略過了哪些：模型是部分解時照著重解。
    let skipped = skipped_by(&model);
    let human = if rules.skip == skipped {
        human
    } else {
        rules.skip = skipped.clone();
        solve_human(&chart, config, &rules)
    };
    response
        .diagnostics
        .extend(solver::skipped_diagnostics(&chart, &skipped));

    let model = match model {
        Ok(solutions) => solutions.into_iter().next(),
        Err(diagnostic) => {
            response.diagnostics.push(diagnostic);
            None
        }
    };
    if let Some(model) = &model {
        response.model = Some(evaluated(model));
        for (index, mark) in &marks {
            let note = &chart.notes[*index];
            let (contact, track) = model_hands(model, &note.id);
            let mut compare =
                |part: &str, human: TrackHand, either: bool, model: Option<TrackHand>| {
                    let Some(model) = model else {
                        return;
                    };
                    response.compared += 1;
                    if either || human == model {
                        response.agreed += 1;
                    } else {
                        response.divergences.push(Divergence {
                            note_id: note.id.clone(),
                            key: note.key.clone(),
                            time_seconds: note.time_seconds,
                            part: part.into(),
                            human,
                            model,
                        });
                    }
                };
            // 「確定」的那一步比手；「皆可」不論模型用哪隻手都算吻合；「不確定」不比。
            let counted = |c: Confidence| c != Confidence::Unsure;
            if let Some(hand) = mark.hand.filter(|_| counted(mark.confidence)) {
                let either = mark.confidence == Confidence::Either;
                compare("hand", as_track(hand), either, contact.map(as_track));
            }
            if let Some(track_hand) = mark.track.filter(|_| counted(mark.slide_confidence())) {
                let either = mark.slide_confidence() == Confidence::Either;
                compare("track", track_hand, either, track);
            }
        }
    }

    let (human, skipped) = human;
    let any_skipped = !skipped.is_empty();
    response.diagnostics.extend(skipped);
    match human {
        Ok(solutions) => {
            if let Some(best) = solutions.into_iter().next() {
                response.human = Some(evaluated(&best));
                response.human_solution = Some(best);
            }
            response.status = match (model.is_some(), any_skipped) {
                (false, _) => "no_solution",
                (true, true) => "human_infeasible",
                (true, false) => "ok",
            }
            .into();
        }
        Err(mut diagnostic) => {
            // 模型自己解得出來、照標註卻在某處全部走不下去：原因在標註（或模型不支援的打法），
            // 不是模型本身無解，改用標註走不通的說明。
            if model.is_some() && diagnostic.code == "no_solution" {
                diagnostic.code = "annotation_infeasible".into();
                diagnostic.message = solver::ANNOTATION_INFEASIBLE_MESSAGE.into();
            }
            response.status = if diagnostic.code == "annotation_infeasible" {
                "human_infeasible".into()
            } else {
                diagnostic.code.clone()
            };
            response.diagnostics.push(diagnostic);
        }
    }
    response
}
