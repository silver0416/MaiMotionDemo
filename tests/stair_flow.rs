//! stairFlow（2026-11-07）：起步那一側的手照固定節奏接著打快速階梯的下一格時，鍵位歸屬與
//! homeEntry 不收，讓一隻手繞完整圈、跨進另一半。新 30 首 +119 步，驗證 49 首 +12、
//! eval18 −8（言ノ葉カルマ那種舊譜照半邊分手）。內建版本的權重都不開；使用者決定做成設定的
//! 開關 `stairFlow`，App 參數頁預設打開，Rust 的 SolverConfig 預設關閉。
use mai_motion_core::scoring_v3::{variant_tuning, with_tuning, TuningV3, V3_VARIANTS};
use mai_motion_core::*;

/// V4.1 加上 stairFlow 之後，每顆音符第一次接觸的手，依時間（同時刻依按鍵）。
fn hands(source: &str, flow: f64) -> Vec<(u8, Hand)> {
    let tuning: &'static TuningV3 = Box::leak(Box::new(TuningV3 {
        stair_flow: flow,
        stair_flow_gap: 0.10,
        ..variant_tuning("v4.1").unwrap().clone()
    }));
    let chart = parse_chart(source, 0.0).unwrap().chart;
    let response = with_tuning(tuning, || {
        analyze_chart(AnalyzeRequest {
            request_id: "t".into(),
            source: source.into(),
            first_seconds: 0.0,
            solver_config: SolverConfig::v3(),
        })
    });
    assert_eq!(response.status, "ok", "{:?}", response.diagnostics);
    let solution = &response.solutions[0];
    let mut notes: Vec<&Note> = chart.notes.iter().collect();
    notes.sort_by(|a, b| {
        a.time_seconds
            .total_cmp(&b.time_seconds)
            .then(a.button.cmp(&b.button))
    });
    notes
        .iter()
        .map(|n| {
            let hand = solution
                .assignments
                .iter()
                .find(|a| a.note_id == n.id && matches!(a.part.as_str(), "contact" | "head"))
                .unwrap()
                .hand;
            (n.button, hand)
        })
        .collect()
}

#[test]
fn stair_flow_off_in_all_variants() {
    let t = TuningV3::default();
    assert_eq!((t.stair_flow, t.stair_flow_gap), (0.0, 0.15));
    for id in V3_VARIANTS {
        assert_eq!(variant_tuning(id).unwrap().stair_flow, 0.0, "{id}");
    }
    assert!(!SolverConfig::default().stair_flow);
    assert!(!SolverConfig::v3().stair_flow);
}

/// 設定的開關：只有 V3 接受，序列化帶出、舊資料沒有這個欄位時是關閉。
#[test]
fn stair_flow_switch_is_v3_only() {
    let on: SolverConfig =
        serde_json::from_str(r#"{"scoringModel":"human-motion-v3","stairFlow":true}"#).unwrap();
    assert!(on.stair_flow);
    assert_eq!(serde_json::to_value(&on).unwrap()["stairFlow"], true);
    let old: SolverConfig = serde_json::from_str(r#"{"scoringModel":"human-motion-v3"}"#).unwrap();
    assert!(!old.stair_flow);
    assert!(serde_json::from_str::<SolverConfig>(
        r#"{"scoringModel":"hand-affinity-v2","stairFlow":true}"#
    )
    .is_err());
    let v2 = SolverConfig {
        stair_flow: true,
        ..SolverConfig::v2()
    };
    assert!(v2.validate().is_err());
    assert!(serde_json::to_value(SolverConfig::v2())
        .unwrap()
        .get("stairFlow")
        .is_none());
}

/// 火炎地獄（DX MASTER）96 秒那種：兩個和弦之間 32 分逆時針繞兩圈多。
const KAEN: &str = "(114){4}1/5,{32}7,6,5,4,3,2,1,8,7,6,5,4,3,2,1,8,7,6,5,{4}1/5,,,E";

/// V4.1 每半圈換一次手；權重打開 stairFlow 後，從左半起步的左手一路刷完（標記者也是左手）。
#[test]
fn stair_flow_keeps_the_runner() {
    let src = KAEN;
    let run = |flow: f64| hands(src, flow)[2..21].to_vec();
    assert!(
        run(0.0).iter().any(|(_, h)| *h == Hand::R),
        "{:?}",
        run(0.0)
    );
    assert!(
        run(1.0).iter().all(|(_, h)| *h == Hand::L),
        "{:?}",
        run(1.0)
    );
}

/// App 的開關：選 V4.1、設定打開 stairFlow，和權重打開的結果一樣是左手刷完。
#[test]
fn stair_flow_switch_keeps_the_runner() {
    let chart = parse_chart(KAEN, 0.0).unwrap().chart;
    let mut solver_config = SolverConfig::v3();
    solver_config.v3_variant = Some("v4.1".into());
    solver_config.stair_flow = true;
    let response = analyze_chart(AnalyzeRequest {
        request_id: "t".into(),
        source: KAEN.into(),
        first_seconds: 0.0,
        solver_config,
    });
    assert_eq!(response.status, "ok", "{:?}", response.diagnostics);
    let solution = &response.solutions[0];
    let mut notes: Vec<&Note> = chart.notes.iter().collect();
    notes.sort_by(|a, b| {
        a.time_seconds
            .total_cmp(&b.time_seconds)
            .then(a.button.cmp(&b.button))
    });
    let run: Vec<Hand> = notes[2..21]
        .iter()
        .map(|n| {
            solution
                .assignments
                .iter()
                .find(|a| a.note_id == n.id && matches!(a.part.as_str(), "contact" | "head"))
                .unwrap()
                .hand
        })
        .collect();
    assert!(run.iter().all(|h| *h == Hand::L), "{run:?}");
    assert_eq!(
        serde_json::to_value(&solution.config_snapshot).unwrap()["stairFlow"],
        true
    );
}
