mod geometry;
mod model;
mod parser;
pub mod scoring;
pub mod scoring_v3;
mod solver;

pub use model::*;
pub use parser::{parse_chart, ParseOutput};

pub fn analyze_chart(request: AnalyzeRequest) -> AnalyzeResponse {
    let mut response = AnalyzeResponse {
        schema_version: if request.solver_config.is_v3() {
            4
        } else if request.solver_config.is_v2() {
            3
        } else {
            2
        },
        request_id: request.request_id,
        status: "invalid".into(),
        diagnostics: vec![],
        chart: None,
        solutions: vec![],
    };
    if let Err(message) = request.solver_config.validate() {
        response
            .diagnostics
            .push(Diagnostic::plain("invalid_config", message));
        return response;
    }
    match parse_chart(&request.source, request.first_seconds) {
        Err(diagnostic) => {
            response.status = if diagnostic.code == "unsupported" {
                "unsupported"
            } else {
                "invalid"
            }
            .into();
            response.diagnostics.push(diagnostic);
        }
        Ok(parsed) => {
            let chart = parsed.chart;
            response.diagnostics.extend(parsed.notices);
            match solver::solve(&chart, &request.solver_config) {
                Ok(solutions) => {
                    response.status = "ok".into();
                    response.solutions = solutions;
                }
                Err(diagnostic) => {
                    response.status = diagnostic.code.clone();
                    response.diagnostics.push(diagnostic);
                }
            }
            response.chart = Some(chart);
        }
    }
    response
}
