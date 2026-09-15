mod geometry;
mod model;
mod parser;
mod solver;

pub use model::*;
pub use parser::parse_chart;

pub fn analyze_chart(request: AnalyzeRequest) -> AnalyzeResponse {
    let mut response = AnalyzeResponse {
        schema_version: 1,
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
        Ok(chart) => {
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
