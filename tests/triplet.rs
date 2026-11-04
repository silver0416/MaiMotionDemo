//! tripletAlt：自動分組剛好三顆、等間隔又快的一組不交互時收成本（標記者多半交互）。
//! 預設 0（V3、V3.1、V3.2 結果不變）；V4 開 1、只算頭尾同鍵（2026-10-31 併入）。
use mai_motion_core::scoring_v3::{variant_tuning, with_tuning, TuningV3};
use mai_motion_core::*;

fn request(source: &str) -> AnalyzeRequest {
    AnalyzeRequest {
        request_id: "t".into(),
        source: source.into(),
        first_seconds: 0.0,
        solver_config: SolverConfig::v3(),
    }
}

/// V4 加上 tripletAlt 之後，每顆音符第一次接觸的手，依 key。
fn hands(source: &str, weight: f64, only_return: bool) -> Vec<(String, Hand)> {
    let chart = parse_chart(source, 0.0).unwrap().chart;
    let tuning: &'static TuningV3 = Box::leak(Box::new(TuningV3 {
        triplet_alt: weight,
        triplet_return: if only_return { 1.0 } else { 0.0 },
        ..variant_tuning("v4").unwrap().clone()
    }));
    let response = with_tuning(tuning, || analyze_chart(request(source)));
    let solution = &response.solutions[0];
    chart
        .notes
        .iter()
        .map(|n| {
            let hand = solution
                .assignments
                .iter()
                .find(|a| a.note_id == n.id && matches!(a.part.as_str(), "contact" | "head"))
                .unwrap()
                .hand;
            (n.key.clone(), hand)
        })
        .collect()
}

fn pattern(hands: &[(String, Hand)], keys: [&str; 3]) -> [Hand; 3] {
    keys.map(|k| hands.iter().find(|(key, _)| key == k).unwrap().1)
}

#[test]
fn triplet_only_in_v4() {
    assert_eq!(TuningV3::default().triplet_alt, 0.0);
    for id in ["v3.1", "v3.2"] {
        assert_eq!(variant_tuning(id).unwrap().triplet_alt, 0.0, "{id}");
    }
    let v4 = variant_tuning("v4").unwrap();
    assert_eq!(
        (v4.triplet_alt, v4.triplet_return, v4.triplet_window),
        (1.0, 1.0, 0.13)
    );
}

/// Entrance（DX MASTER）1:23 那段：12 分一路三顆一組，6-5-6 之後右手要馬上回 3。
/// V4 讓左手自己來回 6-5-6；開 tripletAlt 後交互（L-R-L）。
#[test]
fn triplet_alternates_entrance_passage() {
    const SRC: &str =
        "(180){12}6,5,7,2,1,3,7,8,7,3,4,2,6,5,6,3,1,3,{24}1,8,7,6,5,4,8,1,2,3,4,5,{4},E";
    let keys = ["1.333|tap|6", "1.444|tap|5", "1.556|tap|6"];
    let plain = pattern(&hands(SRC, 0.0, false), keys);
    assert!(plain[0] == plain[1] && plain[1] == plain[2], "{plain:?}");
    let alt = pattern(&hands(SRC, 2.0, true), keys);
    assert!(alt[0] == alt[2] && alt[0] != alt[1], "{alt:?}");
}
