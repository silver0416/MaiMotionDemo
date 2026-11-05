//! 同組同時音的展開順序被浮點誤差打亂時，不能因此整組無解。
use mai_motion_core::*;

/// 2-5／6-1 開始滑行的同一刻，起點上各有一顆新星星與兩個 Touch。
/// 前面空 606 個八分音符後，Slide 起點（頭時間加等待）比新音符時間晚約 1e-14 秒；
/// 任務排序直接比較浮點數，把兩條 Slide 排到 Tap 之後：手先被 Tap 佔住，路徑上的 Touch
/// 只能交給另一手，整組找不到解。現在整組無解時會改用標準順序（同按鍵 Slide 先）重試。
fn chart() -> String {
    format!(
        "(191){{8}}{}{{8}}2-5[8:1]/6-1[8:1],,{{1}}2<3V57[2:3]/6>7V13[2:3]/B2/E3/B6/E7f,,E",
        ",".repeat(606)
    )
}

#[test]
fn slide_start_and_new_star_on_same_button_late_in_chart() {
    let response = analyze_chart(AnalyzeRequest {
        request_id: "order".into(),
        source: chart(),
        first_seconds: 0.0,
        solver_config: SolverConfig::v3(),
    });
    assert_eq!(response.status, "ok", "{:?}", response.diagnostics);
    let chart = response.chart.unwrap();
    let id = |text: &str| {
        chart
            .notes
            .iter()
            .find(|n| n.key.ends_with(&format!("|{text}")))
            .unwrap_or_else(|| panic!("{text}"))
            .id
            .clone()
    };
    let top = &response.solutions[0];
    let hand = |note: &str, part: &str| {
        top.assignments
            .iter()
            .find(|a| a.note_id == note && a.part == part)
            .unwrap_or_else(|| panic!("{note} {part}"))
            .hand
    };
    // 起點上的新星星由正在接 Slide 的同一隻手順帶碰到。
    let (slide, star) = (id("2-5"), id("2<V7"));
    assert_eq!(hand(&slide, "slide"), hand(&star, "head"));
}

/// Now or Never 的寫法：等待時間用秒數寫（取到小數第 4 位），1-5 比同一鍵上的 Tap 早 40 微秒出發，
/// 兩者被拆成兩個時刻。另一手按住 8，出發的手要同一次按下 1 鍵；差距遠小於 Tap 的 ±1 幀。
#[test]
fn slide_starting_microseconds_before_a_tap_on_its_button() {
    for config in [SolverConfig::default(), SolverConfig::v3()] {
        let response = analyze_chart(AnalyzeRequest {
            request_id: "order".into(),
            source: "(120){4}1-5[0.49996##0.5]/8h[1:1],1,E".into(),
            first_seconds: 0.0,
            solver_config: config,
        });
        assert_eq!(response.status, "ok", "{:?}", response.diagnostics);
        let top = &response.solutions[0];
        let hands: Vec<Hand> = top
            .assignments
            .iter()
            .filter(|a| a.part == "slide" || a.start_seconds > 0.4)
            .map(|a| a.hand)
            .collect();
        assert!(
            hands.windows(2).all(|p| p[0] == p[1]),
            "{:?}",
            top.assignments
        );
    }
}
