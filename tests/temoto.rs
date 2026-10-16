//! Temoto 模型：Rust 推論必須與 PyTorch 一致，整條流程的結果動作上做得到。
use mai_motion_core::temoto::{features, TemotoModel, MODEL_IDS};
use mai_motion_core::*;

const CHART: &str = include_str!("charts/speed_first_full.txt");

fn request(ml: Option<&str>) -> AnalyzeRequest {
    let mut solver_config = SolverConfig::v3();
    solver_config.ml_model = ml.map(Into::into);
    AnalyzeRequest {
        request_id: "temoto".into(),
        source: CHART.into(),
        first_seconds: 0.0,
        solver_config,
    }
}

#[test]
fn builtin_models_load() {
    for id in MODEL_IDS {
        let model = TemotoModel::builtin(id).expect(id);
        assert_eq!(model.name, id);
    }
    assert!(TemotoModel::builtin("temoto-9").is_none());
    // akane 附帶 V3.1 的權重，tokiwa 用出貨的 V3
    assert!(TemotoModel::builtin("temoto-1-akane")
        .unwrap()
        .tuning()
        .is_some());
    assert!(TemotoModel::builtin("temoto-1-tokiwa")
        .unwrap()
        .tuning()
        .is_none());
}

/// 參考機率由訓練程式以同一組 f16 權重算出（J:\MaiMotionML\export_model.py --ref）。
#[test]
fn inference_matches_pytorch() {
    let reference: serde_json::Value =
        serde_json::from_str(include_str!("charts/temoto_tokiwa_speed_first_ref.json")).unwrap();
    let chart = parse_chart(CHART, 0.0).unwrap().chart;
    let v3 = analyze_chart(request(None));
    let input = features(&chart, v3.solutions.first());
    let keys: Vec<&str> = input
        .order
        .iter()
        .map(|&i| chart.notes[i].key.as_str())
        .collect();
    let want_keys: Vec<&str> = reference["keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k.as_str().unwrap())
        .collect();
    assert_eq!(keys, want_keys, "音符排序與訓練資料不同");
    let p = TemotoModel::builtin("temoto-1-tokiwa")
        .unwrap()
        .predict(&input);
    for (i, row) in reference["p"].as_array().unwrap().iter().enumerate() {
        for c in 0..2 {
            let want = row[c].as_f64().unwrap() as f32;
            assert!(
                (p[i][c] - want).abs() < 1e-4,
                "{} 第 {c} 項：{} ≠ {want}",
                keys[i],
                p[i][c]
            );
        }
    }
}

#[test]
fn ml_model_corrects_and_stays_feasible() {
    for id in MODEL_IDS {
        let response = analyze_chart(request(Some(id)));
        assert_eq!(response.status, "ok", "{id}: {:?}", response.diagnostics);
        let top = &response.solutions[0];
        assert!(
            top.warnings[0].starts_with("Temoto-1"),
            "{id}: {}",
            top.warnings[0]
        );
        assert!(
            top.warnings[0].contains("修改了"),
            "{id}: {}",
            top.warnings[0]
        );
        assert_eq!(top.config_snapshot.ml_model.as_deref(), Some(id));
    }
}

#[test]
fn ml_model_is_v3_only_and_validated() {
    let mut c = SolverConfig::v2();
    c.ml_model = Some("temoto-1-tokiwa".into());
    assert!(c.validate().is_err());
    let mut c = SolverConfig::v3();
    c.ml_model = Some("temoto-9".into());
    assert!(c.validate().is_err());
    // wire：V3 接受 mlModel，省略時為 None 且不輸出
    let c: SolverConfig =
        serde_json::from_str(r#"{"scoringModel":"human-motion-v3","mlModel":"temoto-1-akane"}"#)
            .unwrap();
    assert_eq!(c.ml_model.as_deref(), Some("temoto-1-akane"));
    let plain = serde_json::to_value(SolverConfig::v3()).unwrap();
    assert!(plain.get("mlModel").is_none());
    assert!(serde_json::from_str::<SolverConfig>(
        r#"{"scoringModel":"hand-affinity-v2","mlModel":"temoto-1-akane"}"#
    )
    .is_err());
}
