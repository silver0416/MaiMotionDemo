//! Debug probe: prints V3 candidates as per-note hands for inline charts or a
//! chart file. Usage: v3_probe [--file path] [--from n --to n] [source...]
use mai_motion_core::*;
fn main() {
    let mut args = std::env::args().skip(1);
    let mut sources = vec![];
    let (mut from, mut to) = (0usize, usize::MAX);
    let mut trace = false;
    let mut config = SolverConfig::v3();
    while let Some(a) = args.next() {
        match a.as_str() {
            "--file" => sources.push(std::fs::read_to_string(args.next().unwrap()).unwrap()),
            "--from" => from = args.next().unwrap().parse().unwrap(),
            "--to" => to = args.next().unwrap().parse().unwrap(),
            "--trace" => trace = true,
            "--comfort" => config.travel_comfort = args.next().unwrap().parse().unwrap(),
            "--home" => config.home_preference = args.next().unwrap().parse().unwrap(),
            "--beam" => config.beam_width = args.next().unwrap().parse().unwrap(),
            _ => sources.push(a),
        }
    }
    for source in sources {
        let start = std::time::Instant::now();
        let r = analyze_chart(AnalyzeRequest {
            request_id: "probe".into(),
            source: source.clone(),
            first_seconds: 0.,
            solver_config: config.clone(),
        });
        let label: String = source.chars().take(60).collect();
        println!(
            "== {} status={} {:.0}ms",
            label.replace('\n', " "),
            r.status,
            start.elapsed().as_secs_f64() * 1000.
        );
        let Some(chart) = r.chart.as_ref() else {
            println!("{:?}", r.diagnostics);
            continue;
        };
        if trace {
            for line in trace_v3(chart, &config).unwrap() {
                println!("  | {line}");
            }
        }
        if let Some(s) = r.solutions.first() {
            // Posture from real trajectories, sampled every 10 ms: swapped means
            // L at x > 0.2 while R at x < -0.2 (both clearly on the other side).
            let at = |segs: &[MotionSegment], t: f64| {
                let seg = segs
                    .iter()
                    .find(|g| g.start_seconds <= t && t <= g.end_seconds)?;
                let k = seg
                    .samples
                    .partition_point(|p| p.time_seconds < t)
                    .clamp(1, seg.samples.len() - 1);
                let (a, b) = (&seg.samples[k - 1], &seg.samples[k]);
                let u = if b.time_seconds > a.time_seconds {
                    ((t - a.time_seconds) / (b.time_seconds - a.time_seconds)).clamp(0., 1.)
                } else {
                    1.
                };
                Some(a.point().lerp(b.point(), u))
            };
            let end = chart
                .notes
                .iter()
                .map(|n| n.end_seconds.max(n.motion_end.unwrap_or(n.time_seconds)))
                .fold(0., f64::max);
            let (mut t, mut swapped, mut run, mut runs) =
                (s.left_segments[0].start_seconds, 0., 0., vec![]);
            let mut started = 0.;
            while t < end {
                let crossed = matches!((at(&s.left_segments, t), at(&s.right_segments, t)), (Some(l), Some(r)) if l.x > 0.2 && r.x < -0.2);
                if crossed {
                    if run == 0. {
                        started = t;
                    }
                    swapped += 0.01;
                    run += 0.01;
                } else if run > 0. {
                    runs.push((run, started));
                    run = 0.;
                }
                t += 0.01;
            }
            if run > 0. {
                runs.push((run, started));
            }
            runs.sort_by(|a: &(f64, f64), b| b.0.total_cmp(&a.0));
            let long = runs.iter().filter(|r| r.0 >= 1.0).count();
            println!(
                "  posture: swapped {swapped:.1}s in {} episodes, >=1s: {long}, longest {:?}",
                runs.len(),
                runs.iter()
                    .take(5)
                    .map(|r| format!("{:.2}s@{:.2}", r.0, r.1))
                    .collect::<Vec<_>>()
            );
        }
        for s in &r.solutions {
            let hands: Vec<String> = chart
                .notes
                .iter()
                .enumerate()
                .filter(|(i, _)| *i >= from && *i <= to)
                .map(|(_, n)| {
                    let h = s
                        .assignments
                        .iter()
                        .filter(|a| a.note_id == n.id && a.part != "slide" && a.part != "group")
                        .map(|a| if a.hand == Hand::L { "L" } else { "R" })
                        .collect::<String>();
                    format!(
                        "{}:{}{}@{:.3}={}",
                        n.id,
                        if n.touch_area.is_some() { "T" } else { "" },
                        n.button,
                        n.time_seconds,
                        h
                    )
                })
                .collect();
            println!("  {} {}", s.id, hands.join(" "));
            println!("    {}", serde_json::to_string(&s.score_breakdown).unwrap());
        }
    }
}
