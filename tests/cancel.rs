use mai_motion_core::*;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

fn request() -> AnalyzeRequest {
    AnalyzeRequest {
        request_id: "cancel-test".into(),
        source: "(120){4}1,2,3,4,5,6,7,8,E".into(),
        first_seconds: 0.,
        solver_config: SolverConfig::v3(),
    }
}

#[test]
fn a_raised_flag_stops_the_search() {
    let response = with_cancel(Arc::new(AtomicBool::new(true)), || analyze_chart(request()));
    assert_eq!(response.status, "cancelled");
    assert!(response.solutions.is_empty());
}

#[test]
fn a_lowered_flag_changes_nothing_and_is_cleared_afterwards() {
    let response = with_cancel(
        Arc::new(AtomicBool::new(false)),
        || analyze_chart(request()),
    );
    assert_eq!(response.status, "ok");
    // 旗標只在 with_cancel 期間有效，之後的分析不受影響。
    with_cancel(Arc::new(AtomicBool::new(true)), || ());
    assert_eq!(analyze_chart(request()).status, "ok");
}
