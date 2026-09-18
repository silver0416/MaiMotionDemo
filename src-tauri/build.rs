fn main() {
    // 發佈通路：portable（預設，免安裝單一 exe）或 installed（未來的安裝版）。
    // 安裝版 CI 建置時設 MAIMOTION_DISTRIBUTION=installed；本機預設即 portable。
    let distribution = std::env::var("MAIMOTION_DISTRIBUTION").unwrap_or_else(|_| "portable".into());
    let distribution = distribution.trim().to_lowercase();
    let distribution = if distribution == "installed" { "installed" } else { "portable" };
    println!("cargo:rustc-env=MAIMOTION_DISTRIBUTION={distribution}");
    tauri_build::build();
}
