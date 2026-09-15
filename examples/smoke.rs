use mai_motion_core::*;

fn main() {
    let cases: Vec<(&str, &str)> = vec![
        ("tap", "(120){4}1,2,3,4,E"),
        ("each-slash", "(120){4}1/5,E"),
        ("each-concat", "(120){4}15,E"),
        ("break-ex", "(120){4}1b,2x,3bx,E"),
        ("star", "(120){4}1$,2$$,E"),
        ("hold", "(120){4}1h[4:1],2h[#0.5],3h[240#4:1],E"),
        ("hold-break", "(120){4}1bh[4:1],1h[4:1]b,E"),
        ("touch", "(120){4}A1,B3,C,C1,D5,E7,E"),
        ("touch-hold", "(120){4}Ch[4:1],C1h[#1]f,E"),
        ("touch-firework", "(120){4}A2f,E"),
        ("slide-line", "(120){4}1-5[4:1],E"),
        ("slide-arc", "(120){4}1^3[4:1],1<5[4:1],1>5[4:1],E"),
        ("slide-v", "(120){4}1v5[4:1],1v4[4:1],E"),
        ("slide-grand-v", "(120){4}1V73[4:1],E"),
        (
            "slide-pq",
            "(120){4}1p5[4:1],1q5[4:1],1pp5[4:1],1qq5[4:1],E",
        ),
        ("slide-sz", "(120){4}1s5[4:1],1z5[4:1],E"),
        ("slide-wifi", "(120){4}1w5[4:1],E"),
        ("slide-chain-one-len", "(120){4}1-3-5[4:1],E"),
        ("slide-chain-each-len", "(120){4}1-3[8:1]-5[8:1],E"),
        ("slide-multi", "(120){4}1-3[4:1]*-7[4:1],E"),
        ("slide-nohead", "(120){4}1?-5[4:1],1!-5[4:1],E"),
        ("slide-break", "(120){4}1-5[4:1]b,E"),
        (
            "slide-times",
            "(120){4}1-5[#1.5],1-5[160#8:1],1-5[1##2],1-5[##1.5],1-5[1##8:1],E",
        ),
        ("pseudo-each", "(120){4}1`5`3,E"),
        ("comment", "(120){4}1,2,||這是註解\n3,4,E"),
        ("bpm-mid", "(120){4}1,2,(240){8}3,4,E"),
        ("fixed-step", "{#0.25}1,2,3,E"),
        (
            "maidata",
            "&title=demo\n&first=1.5\n&inote_2=(120){4}1,2,E\n&inote_5=(120){4}1-5[4:1],3,E\n",
        ),
    ];
    let mut bad = 0;
    for (name, source) in cases {
        match parse_chart(source, 0.0) {
            Ok(parsed) => {
                let notes = parsed.chart.notes.len();
                let paths = parsed
                    .chart
                    .paths
                    .iter()
                    .map(|p| p.shape.clone())
                    .collect::<Vec<_>>()
                    .join(" ");
                let notices = parsed
                    .notices
                    .iter()
                    .map(|d| d.message.clone())
                    .collect::<Vec<_>>()
                    .join(" | ");
                println!("{name:24} ok    notes={notes:3} paths=[{paths}] {notices}");
            }
            Err(d) => {
                bad += 1;
                println!("{name:24} FAIL  {} {}", d.code, d.message);
            }
        }
    }
    let errors: Vec<&str> = vec![
        "(120){4}1-2[4:1],E",
        "(120){4}1^5[4:1],E",
        "(120){4}1V83[4:1],E",
        "(120){4}1s3[4:1],E",
        "(120){4}1w3[4:1],E",
        "(120){4}1-5,E",
        "(120){4}A9,E",
        "(120){4}1,",
        "(120){4}1k5[4:1],E",
    ];
    for source in errors {
        let r = analyze_chart(AnalyzeRequest {
            request_id: "err".into(),
            source: source.into(),
            first_seconds: 0.0,
            solver_config: SolverConfig::default(),
        });
        if r.status == "ok" {
            bad += 1;
            println!("SHOULD FAIL BUT OK: {source}");
        } else {
            println!("{:32} -> {} {}", source, r.status, r.diagnostics[0].message);
        }
    }
    println!("failures={bad}");
}
