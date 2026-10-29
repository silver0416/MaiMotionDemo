//! Temoto 模型：Rust 推論必須與 PyTorch 一致，整條流程的結果動作上做得到。
use mai_motion_core::scoring_v3::{variant_tuning, with_tuning, TuningV3, V3_VARIANTS};
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
    // 改動門檻：tokiwa 要把握 0.6 才改，akane 0.5
    assert_eq!(
        TemotoModel::builtin("temoto-1-tokiwa").unwrap().flip_at(),
        0.6
    );
    assert_eq!(
        TemotoModel::builtin("temoto-1-akane").unwrap().flip_at(),
        0.5
    );
    // ruri：分組架構，附帶 V3.2 的權重，門檻 0.5
    let ruri = TemotoModel::builtin("temoto-1-ruri").unwrap();
    assert!(ruri.grouped());
    assert!(!TemotoModel::builtin("temoto-1-akane").unwrap().grouped());
    assert_eq!(ruri.flip_at(), 0.5);
    assert_eq!(
        serde_json::to_value(ruri.tuning().unwrap()).unwrap(),
        serde_json::to_value(variant_tuning("v3.2").unwrap()).unwrap()
    );
    // rurikon（ruri 的網路）與 ayame（V4 重訓）：分組架構，附帶 V4 的權重（含 touchPalm），門檻 0.5
    for id in ["temoto-1-rurikon", "temoto-1-ayame"] {
        let model = TemotoModel::builtin(id).unwrap();
        assert!(model.grouped(), "{id}");
        assert_eq!(model.flip_at(), 0.5, "{id}");
        assert_eq!(
            serde_json::to_value(model.tuning().unwrap()).unwrap(),
            serde_json::to_value(variant_tuning("v4").unwrap()).unwrap(),
            "{id}"
        );
    }
}

/// 提醒文字裡的基座名稱照模型附帶的權重，不是一律寫 V3。
#[test]
fn base_label_follows_model_tuning() {
    for (id, label) in [
        ("temoto-1-tokiwa", "V3"),
        ("temoto-1-akane", "V3.1"),
        ("temoto-1-ruri", "V3.2"),
        ("temoto-1-rurikon", "V4"),
        ("temoto-1-ayame", "V4"),
    ] {
        assert_eq!(
            TemotoModel::builtin(id).unwrap().base_label(),
            label,
            "{id}"
        );
    }
}

/// 分組架構（ruri）：參考機率由 J:\MaiMotionML\g1\export_group.py --ref 以同一組 f16 權重算出。
/// 參考的輸入是模型附帶的 V3.2 權重下的手順與自動分組；這首超過一個推論視窗，切段也一起比對。
#[test]
fn group_inference_matches_pytorch() {
    let reference: serde_json::Value =
        serde_json::from_str(include_str!("charts/temoto_ruri_speed_first_ref.json")).unwrap();
    let reference = &reference[0];
    let model = TemotoModel::builtin("temoto-1-ruri").unwrap();
    let chart = parse_chart(CHART, 0.0).unwrap().chart;
    let input = with_tuning(model.tuning().unwrap(), || {
        let v3 = analyze_chart(request(None));
        features(&chart, v3.solutions.first())
    });
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
    assert_eq!(
        input.gdesc.len() as u64,
        reference["groups"].as_u64().unwrap(),
        "自動分組數不同"
    );
    assert!(keys.len() > 256, "要超過一個推論視窗才測得到切段");
    let p = model.predict(&input);
    for (i, row) in reference["p"].as_array().unwrap().iter().enumerate() {
        for c in 0..2 {
            let want = row[c].as_f64().unwrap() as f32;
            assert!(
                (p[i][c] - want).abs() < 1e-5,
                "{} 第 {c} 項：{} ≠ {want}",
                keys[i],
                p[i][c]
            );
        }
    }
}

/// 參考機率由訓練程式以同一組 f16 權重算出（J:\MaiMotionML\export_model.py --ref）。
/// 參考的輸入是當時的 V3 手順（還沒有自動分組），所以這裡關掉 groupAuto 重現同一份輸入。
#[test]
fn inference_matches_pytorch() {
    let reference: serde_json::Value =
        serde_json::from_str(include_str!("charts/temoto_tokiwa_speed_first_ref.json")).unwrap();
    let chart = parse_chart(CHART, 0.0).unwrap().chart;
    let then: &'static TuningV3 = Box::leak(Box::new(TuningV3 {
        group_auto: 0.0,
        ..TuningV3::default()
    }));
    let v3 = with_tuning(then, || analyze_chart(request(None)));
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

/// V3 變體：V3.1 是重新擬合的權重，V3.2 只再把 starSwitch 降到 0.1。
#[test]
fn v3_variants_are_builtin_tunings() {
    let v31 = variant_tuning("v3.1").unwrap();
    let v32 = variant_tuning("v3.2").unwrap();
    assert!(variant_tuning("v3.9").is_none());
    assert_eq!(v32.star_switch, 0.1);
    assert!((v31.star_switch - 0.5724).abs() < 1e-3);
    let (mut a, mut b) = (
        serde_json::to_value(v31).unwrap(),
        serde_json::to_value(v32).unwrap(),
    );
    a.as_object_mut().unwrap().remove("starSwitch");
    b.as_object_mut().unwrap().remove("starSwitch");
    assert_eq!(a, b, "V3.2 和 V3.1 只差 starSwitch");
    // V3.1 就是茜附帶的權重
    assert_eq!(
        serde_json::to_value(
            TemotoModel::builtin("temoto-1-akane")
                .unwrap()
                .tuning()
                .unwrap()
        )
        .unwrap(),
        serde_json::to_value(v31).unwrap()
    );
}

#[test]
fn v3_variant_is_validated_and_used() {
    for id in V3_VARIANTS {
        let mut c = SolverConfig::v3();
        c.v3_variant = Some(id.into());
        assert!(c.validate().is_ok(), "{id}");
        // Temoto 模型自帶基座，不能再指定變體
        c.ml_model = Some("temoto-1-akane".into());
        assert!(c.validate().is_err(), "{id}");
    }
    let mut c = SolverConfig::v3();
    c.v3_variant = Some("v3.9".into());
    assert!(c.validate().is_err());
    let mut c = SolverConfig::v2();
    c.v3_variant = Some("v3.1".into());
    assert!(c.validate().is_err());
    // wire：只有 V3 接受，省略時不輸出
    let c: SolverConfig =
        serde_json::from_str(r#"{"scoringModel":"human-motion-v3","v3Variant":"v3.2"}"#).unwrap();
    assert_eq!(c.v3_variant.as_deref(), Some("v3.2"));
    assert!(serde_json::to_value(SolverConfig::v3())
        .unwrap()
        .get("v3Variant")
        .is_none());
    assert!(serde_json::from_str::<SolverConfig>(
        r#"{"scoringModel":"hand-affinity-v2","v3Variant":"v3.2"}"#
    )
    .is_err());
    // 分析時用變體的權重：和直接套 V3.2 權重求解的結果相同
    let mut req = request(None);
    req.solver_config.v3_variant = Some("v3.2".into());
    let with_variant = analyze_chart(req);
    assert_eq!(with_variant.status, "ok", "{:?}", with_variant.diagnostics);
    assert_eq!(
        with_variant.solutions[0]
            .config_snapshot
            .v3_variant
            .as_deref(),
        Some("v3.2")
    );
    let direct = with_tuning(variant_tuning("v3.2").unwrap(), || {
        analyze_chart(request(None))
    });
    let hands = |r: &AnalyzeResponse| {
        r.solutions[0]
            .assignments
            .iter()
            .map(|a| (a.note_id.clone(), a.part.clone(), a.hand))
            .collect::<Vec<_>>()
    };
    assert_eq!(hands(&with_variant), hands(&direct));
}
