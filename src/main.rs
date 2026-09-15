use std::io::{self, Read};
fn main() {
    let mut source = String::new();
    if let Err(e) = io::stdin().take(200_001).read_to_string(&mut source) {
        eprintln!("{e}");
        std::process::exit(1);
    }
    let request: mai_motion_core::AnalyzeRequest = match serde_json::from_str(&source) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Invalid request JSON: {e}");
            std::process::exit(1);
        }
    };
    let response = mai_motion_core::analyze_chart(request);
    println!(
        "{}",
        serde_json::to_string(&response).expect("finite analysis response")
    );
}
