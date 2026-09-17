//! 依 maimai DX 判定規則放寬「手何時可以離開上一個接觸」的行為測試。
use mai_motion_core::*;

const FRAME: f64 = 1.0 / 60.0;

fn run(source: &str, solver_config: SolverConfig) -> AnalyzeResponse {
    analyze_chart(AnalyzeRequest {
        request_id: "judgment".into(),
        source: source.into(),
        first_seconds: 0.0,
        solver_config,
    })
}
fn configs() -> [SolverConfig; 3] {
    [
        SolverConfig::default(),
        SolverConfig::v2(),
        SolverConfig::v3(),
    ]
}
fn ok(source: &str, c: SolverConfig) -> AnalyzeResponse {
    let r = run(source, c);
    assert_eq!(r.status, "ok", "{source}: {:?}", r.diagnostics);
    for s in &r.solutions {
        continuity(&s.left_segments);
        continuity(&s.right_segments);
    }
    r
}
fn continuity(segments: &[MotionSegment]) {
    for pair in segments.windows(2) {
        assert!((pair[0].end_seconds - pair[1].start_seconds).abs() < 1e-7);
        let a = pair[0].samples.last().unwrap().point();
        let b = pair[1].samples[0].point();
        assert!(a.distance(b) < 1e-7, "軌跡不連續");
    }
}
fn note(r: &AnalyzeResponse, pick: impl Fn(&Note) -> bool) -> &Note {
    r.chart
        .as_ref()
        .unwrap()
        .notes
        .iter()
        .find(|n| pick(n))
        .unwrap()
}
fn contacts<'a>(r: &'a AnalyzeResponse, id: &str) -> Vec<&'a Assignment> {
    r.solutions[0]
        .assignments
        .iter()
        .filter(|a| a.note_id == id && a.part != "slide")
        .collect()
}

/// 使用者回報的第 67 行：一手按住 3 號鍵，另一手要在 25ms 間隔內擦過四組 Touch。
#[test]
fn one_hand_wipes_dense_touch_groups_while_the_other_holds() {
    let source = "(200){2}3hx[4:3]/4hx[4:1],{48}A2,A1/B1/D2/E2,A8/B8/D1/E1,A7/B7/D8/E8,A6/B6/D7/E7,,,,,,,,{4},E";
    for c in configs() {
        let r = ok(source, c);
        let a2 = note(&r, |n| {
            n.touch_area.as_deref() == Some("A") && n.button == 2
        });
        let hit = contacts(&r, &a2.id);
        assert!(
            (hit[0].start_seconds - a2.time_seconds).abs() < 1e-9,
            "能準時就不晚接"
        );
        let hand = hit[0].hand;
        let chart = r.chart.as_ref().unwrap();
        for n in chart.notes.iter().filter(|n| n.kind == "touch") {
            let hit = contacts(&r, &n.id);
            assert_eq!(hit.len(), 1);
            assert_eq!(hit[0].hand, hand, "{} 應由擦過的同一隻手完成", n.id);
            assert!((hit[0].start_seconds - n.time_seconds).abs() < 1e-9);
        }
        let segments = match hand {
            Hand::L => &r.solutions[0].left_segments,
            Hand::R => &r.solutions[0].right_segments,
        };
        assert!(segments.iter().filter(|s| s.mode == "glide").count() >= 4);
    }
}

#[test]
fn contact_can_glide_after_one_judgment_frame() {
    // 另一手按住 8；64 分音在 200 BPM 為 18.75ms，長於一幀。
    for c in configs() {
        let r = ok("(200){4}8h[1:1],{64}2,3,4,E", c.clone());
        let solution = &r.solutions[0];
        let tapped: Vec<_> = solution
            .assignments
            .iter()
            .filter(|a| a.note_id != "n0")
            .map(|a| a.hand)
            .collect();
        assert!(tapped.windows(2).all(|p| p[0] == p[1]));
        // 128 分音只有 9.4ms，連一幀都不到，不能靠滑移完成。
        assert_eq!(run("(200){4}8h[1:1],{128}2,3,4,E", c).status, "no_solution");
    }
}

#[test]
fn hold_can_release_within_unchecked_tail() {
    for c in configs() {
        // 兩手按住到 1.0 秒；0.9 秒的雙押在結尾 12 幀（0.2 秒）內，可提早放手。
        let r = ok("(120){4}1h[4:2]/8h[4:2],{20},,,,3/6,E", c.clone());
        let hold = note(&r, |n| n.kind == "hold");
        let segments = [
            &r.solutions[0].left_segments,
            &r.solutions[0].right_segments,
        ];
        let released = segments
            .iter()
            .flat_map(|s| s.iter())
            .find(|s| s.mode == "hold" && s.note_id.as_deref() == Some(&hold.id))
            .unwrap();
        assert!((released.end_seconds - (hold.end_seconds - 12.0 * FRAME)).abs() < 1e-9);
        // 0.7 秒時兩手都還在必須按住的區間。
        assert_eq!(
            run("(120){4}1h[4:2]/8h[4:2],{20},,3/6,E", c.clone()).status,
            "no_solution"
        );
        // 不超過開頭 6 幀＋結尾 12 幀的短 Hold 只看頭判，按一下就能放手。
        ok("(120){4}1h[#0.3]/8h[#0.3],{#0.1}3/6,E", c);
    }
}

#[test]
fn touch_may_be_hit_late_inside_critical_perfect() {
    for c in configs() {
        // 兩手要到 0.8 秒才能放開 Hold；0.75 秒的 Touch 晚接仍在 +9 幀內。
        let r = ok("(120){4}1h[4:2]/8h[4:2],{40},,,,,B3,E", c.clone());
        let touch = note(&r, |n| n.kind == "touch");
        let hit = contacts(&r, &touch.id)[0];
        assert!(hit.start_seconds > touch.time_seconds + 1e-9);
        assert!(hit.start_seconds <= touch.time_seconds + 9.0 * FRAME + 1e-9);
        // Tap 有 Fast/Late，不套用 Touch 的晚接區間。
        assert_eq!(
            run("(120){4}1h[4:2]/8h[4:2],{40},,,,,3,E", c.clone()).status,
            "no_solution"
        );
        // 0.6 秒的 Touch 等到放手已超過 Critical Perfect 區間。
        assert_eq!(
            run("(120){4}1h[4:2]/8h[4:2],{40},,,B3,E", c).status,
            "no_solution"
        );
    }
}

#[test]
fn tracking_hand_brushes_touch_on_its_path() {
    for c in configs() {
        // 1-5 約在 0.634 秒經過 B1，E2 在其旁 0.247，不在路徑上但在順帶碰觸範圍內；另一手按住 8 到 1.5 秒。
        let r = ok("(120){4}1-5[4:1]/8h[4:3],{16},E2,E", c.clone());
        let slide = note(&r, |n| n.kind == "slide");
        let owner = r.solutions[0]
            .assignments
            .iter()
            .find(|a| a.note_id == slide.id && a.part == "slide")
            .unwrap()
            .hand;
        let center = note(&r, |n| n.kind == "touch");
        let hit = contacts(&r, &center.id)[0];
        assert_eq!(hit.hand, owner);
        // 隔一區的 B2 離路徑 0.329，超出順帶碰觸範圍。
        assert_eq!(
            run("(120){4}1-5[4:1]/8h[4:3],{16},B2,E", c.clone()).status,
            "no_solution"
        );
        // 路徑外的 Touch 不會被順帶完成。
        assert_eq!(
            run("(120){4}1-5[4:1]/8h[4:3],{16},A3,E", c.clone()).status,
            "no_solution"
        );
        // 按住外圈按鍵的手在螢幕外，不會碰到旁邊的 A 區。
        assert_eq!(run("(120){4}1h[4:8],Ch[4:4],A1,E", c).status, "no_solution");
    }
}

#[test]
fn touch_group_majority_judges_the_rest() {
    for c in configs() {
        // 一手按住 8；A1 D2 A2 D3 A3 彼此相鄰成 5 顆的 Group，一掌蓋不完全部，
        // 只要實際接觸過半（至少 3 顆），其餘由 Group 連帶判定。
        let r = ok("(120){4}8h[4:3]/A1/D2/A2/D3/A3,E", c.clone());
        let grouped: Vec<_> = r.solutions[0]
            .assignments
            .iter()
            .filter(|a| a.part == "group")
            .collect();
        let palm = &r.solutions[0].palm_placements[0];
        assert!((3..5).contains(&palm.covered_note_ids.len()));
        assert_eq!(grouped.len(), 5 - palm.covered_note_ids.len());
        assert!(grouped.iter().all(|a| a.hand == palm.hand));
        assert!(r.solutions[0]
            .warnings
            .iter()
            .any(|w| w.contains("Touch Group")));
        // A 區彼此不相鄰，沒有 Group 可以連帶判定。
        assert_eq!(
            run("(120){4}8h[4:3]/A1/A2/A3/A4/A5,E", c.clone()).status,
            "no_solution"
        );
        // 9 顆的 Group 需要 5 顆實際接觸；相連 5 顆橫跨 90°，一掌（半徑 0.5）蓋不到。
        assert_eq!(
            run("(120){4}8h[4:3]/A1/D2/A2/D3/A3/D4/A4/D5/A5,E", c).status,
            "no_solution"
        );
    }
}

#[test]
fn slide_judgment_point_depends_on_shape() {
    let progress =
        |source: &str| run(source, SolverConfig::default()).chart.unwrap().paths[0].judge_progress;
    // MajdataPlay SlideTables.cs：line5 Const 0.152、s 0.13、circle3 0.233。
    assert!((progress("(120){4}1-5[4:1],E") - 0.848).abs() < 1e-9);
    assert!((progress("(120){4}1s5[4:1],E") - 0.87).abs() < 1e-9);
    assert!((progress("(120){4}1^3[4:1],E") - 0.767).abs() < 1e-9);
    // 下半圈起點的 < 是順時針，與上半圈的 > 查同一張表。
    assert!((progress("(120){4}5<7[4:1],E") - progress("(120){4}1>3[4:1],E")).abs() < 1e-9);
    // 連續寫法只有最後一段的停留，依弧長換算到整條路徑。
    let chained = progress("(120){4}1-3-5[4:1],E");
    assert!(chained > 0.848 && chained < 1.0);
}

#[test]
fn slide_hand_leaves_after_entering_the_last_judgment_area() {
    for c in configs() {
        // 1-5 在 0.5–1.0 秒移動，約 0.924 秒進入 A5；另一手按住 8。0.95 秒的 4 要同一隻手。
        let source = "(120){4}1-5[4:1]/8h[4:3],{40},,,,,,,,,4,E";
        let r = ok(source, c.clone());
        let chart = r.chart.as_ref().unwrap();
        let slide = &chart.notes[0];
        let judge = 0.5 + 0.5 * chart.paths[0].judge_progress;
        let traced = r.solutions[0]
            .assignments
            .iter()
            .filter(|a| a.note_id == slide.id && a.part == "slide")
            .map(|a| a.end_seconds)
            .fold(0.0, f64::max);
        assert!(
            (traced - judge).abs() < 1e-9,
            "應在進入最後判定區時離手：{traced}"
        );
        assert!(r.solutions[0]
            .warnings
            .iter()
            .any(|w| w.contains("最後判定區")));
        // 0.9 秒還沒進入最後判定區，不能離手。
        assert_eq!(
            run("(120){4}1-5[4:1]/8h[4:3],{40},,,,,,,,4,E", c.clone()).status,
            "no_solution"
        );
        // 沒有衝突時照舊追到終點。
        let free = ok("(120){4}1-5[4:1],E", c);
        let end = free.solutions[0]
            .assignments
            .iter()
            .filter(|a| a.part == "slide")
            .map(|a| a.end_seconds)
            .fold(0.0, f64::max);
        assert!((end - 1.0).abs() < 1e-9);
    }
}
