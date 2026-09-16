//! Slide 形狀方向回歸：路徑依序經過的 A/B/C 判定區必須包含 MajdataPlay 判定佇列
//! （Assets/Scripts/Scenes/Game/Utils/SlideTables.cs）作為子序列。
use mai_motion_core::*;
use std::f64::consts::PI;

/// 以扇形粗分 A/B/C 區，忽略 D/E（Slide 判定不看 D/E）。
fn zone(p: Point) -> String {
    let r = p.x.hypot(p.y);
    if r < 0.2 {
        return "C".into();
    }
    // 第 k 鍵角度 = -π/2 + π/8 + (k-1)π/4；四捨五入到最近的鍵。
    let a = p.y.atan2(p.x) + PI / 2.0 - PI / 8.0;
    let k = ((a / (PI / 4.0)).round() as i32).rem_euclid(8) + 1;
    format!("{}{k}", if r < 0.6 { "B" } else { "A" })
}

fn zones(source: &str) -> Vec<String> {
    let r = analyze_chart(AnalyzeRequest {
        request_id: "geometry".into(),
        source: source.into(),
        first_seconds: 0.0,
        solver_config: SolverConfig::default(),
    });
    assert_eq!(r.status, "ok", "{source}: {:?}", r.diagnostics);
    let path = &r.chart.unwrap().paths[0];
    let mut out: Vec<String> = vec![];
    for w in path.samples.windows(2) {
        for i in 0..20 {
            let t = i as f64 / 20.0;
            let p = Point {
                x: w[0].x + (w[1].x - w[0].x) * t,
                y: w[0].y + (w[1].y - w[0].y) * t,
            };
            let z = zone(p);
            if out.last() != Some(&z) {
                out.push(z);
            }
        }
    }
    out
}

/// queue 的每一段可寫成 "B8|A8" 表示任一區皆可。
fn assert_passes(source: &str, queue: &str) {
    let seen = zones(source);
    let mut cursor = 0;
    for step in queue.split_whitespace() {
        let options: Vec<&str> = step.split('|').collect();
        match seen[cursor..]
            .iter()
            .position(|z| options.contains(&z.as_str()))
        {
            Some(offset) => cursor += offset + 1,
            None => panic!("{source} 應依序經過 {queue}，實際 {seen:?}"),
        }
    }
}

fn rotate(queue: &str, by: u8) -> String {
    queue
        .split_whitespace()
        .map(|step| {
            step.split('|')
                .map(|z| {
                    if z == "C" {
                        return z.to_string();
                    }
                    let k = z[1..].parse::<u8>().unwrap();
                    format!("{}{}", &z[..1], (k - 1 + by) % 8 + 1)
                })
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn s_and_z_bend_like_majdata() {
    assert_passes("(120){4}1s5[4:1],E", "A1 B8 B7 C B3 B4 A5");
    // z 是 s 的鏡射。
    assert_passes("(120){4}1z5[4:1],E", "A1 B2 B3 C B7 B6 A5");
    // 旋轉起點後方向一致。
    assert_passes("(120){4}3s7[4:1],E", &rotate("A1 B8 B7 C B3 B4 A5", 2));
    assert_passes("(120){4}6z2[4:1],E", &rotate("A1 B2 B3 C B7 B6 A5", 5));
}

#[test]
fn p_and_q_loop_around_the_center_like_majdata() {
    assert_passes("(120){4}1p5[4:1],E", "A1 B8 B7 B6 A5");
    assert_passes("(120){4}1p2[4:1],E", "A1 B8 B7 B6 B5 B4 B3 A2");
    assert_passes("(120){4}1p1[4:1],E", "A1 B8 B7 B6 B5 B4 B3 B2 A1");
    // q 是 p 的鏡射。
    assert_passes("(120){4}1q5[4:1],E", "A1 B2 B3 B4 A5");
    assert_passes("(120){4}1q8[4:1],E", "A1 B2 B3 B4 B5 B6 B7 A8");
}

#[test]
fn pp_and_qq_loop_through_the_center_like_majdata() {
    let pp = [
        (1, "A1 B1 C B4 A3 A2 A1"),
        (2, "A1 B1 C B4 A3 A2"),
        (3, "A1 B1 C B4 A3"),
        (4, "A1 B1 C B4 A3 A2 B1 C B4 A4"),
        (5, "A1 B1 C B4 A3 A2 B1 C B5 A5"),
        // Majdata 的出口是曲線，會再掠過 C；本模型以直線切出，只要求繞行側與終點一致。
        (6, "A1 B1 C B4 A3 A2 B1 B7|B6 A6"),
        (7, "A1 B1 C B4 A3 A2 B1 B8 A7"),
        (8, "A1 B1 C B4 A3 A2 B1|A1 A8"),
    ];
    for (end, queue) in pp {
        assert_passes(&format!("(120){{4}}1pp{end}[4:1],E"), queue);
    }
    assert_passes("(120){4}1qq5[4:1],E", "A1 B1 C B6 A7 A8 B1 C B5 A5");
    assert_passes("(120){4}1qq1[4:1],E", "A1 B1 C B6 A7 A8 A1");
    assert_passes(
        "(120){4}4pp8[4:1],E",
        &rotate("A1 B1 C B4 A3 A2 B1 C B5 A5", 3),
    );
}

#[test]
fn grand_v_end_must_be_across_the_turn() {
    let status = |source: &str| {
        analyze_chart(AnalyzeRequest {
            request_id: "v".into(),
            source: source.into(),
            first_seconds: 0.0,
            solver_config: SolverConfig::default(),
        })
        .status
    };
    for ok in ["1V72", "1V75", "1V38", "1V36", "1V35"] {
        assert_eq!(status(&format!("(120){{4}}{ok}[4:1],E")), "ok", "{ok}");
    }
    for bad in ["1V71", "1V78", "1V32", "1V34"] {
        assert_eq!(
            status(&format!("(120){{4}}{bad}[4:1],E")),
            "invalid",
            "{bad}"
        );
    }
}
