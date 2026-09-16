//! Wiki 資料模型與錯誤型別。
//!
//! Level 一律存 `String`（例如 `7+`、`13+`、`14?`），不轉數字，避免解析失敗。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// 譜面種類：Standard（STD）或 Deluxe（DX）。
/// 資料是 simai Wiki 使用者依官方譜面轉錄的文字譜，不是 SEGA 官方 API。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChartType {
    Standard,
    Deluxe,
}

impl ChartType {
    /// 接受 `standard`/`std`、`deluxe`/`dx`（大小寫無關）。
    pub fn from_param(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "standard" | "std" => Some(Self::Standard),
            "deluxe" | "dx" => Some(Self::Deluxe),
            _ => None,
        }
    }
}

/// 六種難度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Difficulty {
    Easy,
    Basic,
    Advanced,
    Expert,
    Master,
    ReMaster,
}

impl Difficulty {
    /// 固定順序，用於排序與顯示。
    pub const ALL: [Self; 6] = [
        Self::Easy,
        Self::Basic,
        Self::Advanced,
        Self::Expert,
        Self::Master,
        Self::ReMaster,
    ];

    /// Index 表格的欄位名。
    pub fn index_header(self) -> &'static str {
        match self {
            Self::Easy => "ESY",
            Self::Basic => "BSC",
            Self::Advanced => "ADV",
            Self::Expert => "EXP",
            Self::Master => "MAS",
            Self::ReMaster => "Re:MAS",
        }
    }

    /// 歌曲頁難度小節的標題。
    pub fn section_heading(self) -> &'static str {
        match self {
            Self::Easy => "EASY",
            Self::Basic => "BASIC",
            Self::Advanced => "ADVANCED",
            Self::Expert => "EXPERT",
            Self::Master => "MASTER",
            Self::ReMaster => "Re:MASTER",
        }
    }

    /// 解析前端傳入的難度參數：大小寫無關，接受 index 欄位名與小節標題。
    pub fn from_param(text: &str) -> Option<Self> {
        match text.trim().to_ascii_uppercase().as_str() {
            "EASY" | "ESY" => Some(Self::Easy),
            "BASIC" | "BSC" => Some(Self::Basic),
            "ADVANCED" | "ADV" => Some(Self::Advanced),
            "EXPERT" | "EXP" => Some(Self::Expert),
            "MASTER" | "MAS" => Some(Self::Master),
            "RE:MASTER" | "REMASTER" | "RE:MAS" => Some(Self::ReMaster),
            _ => None,
        }
    }

    /// 依小節標題文字判斷難度（大小寫無關，接受 index 欄位名）。
    pub fn from_section_text(text: &str) -> Option<Self> {
        Self::from_param(text)
    }
}

/// Index 表格中某個難度的資訊。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiDifficultyInfo {
    /// 難度等級字串（`7+`、`13+`、`14?` 等）；`-` 或空白為 None。
    pub level: Option<String>,
    /// Index 頁面的難度格子是否有連結：只是「可能有譜」的提示，
    /// 最終以歌曲頁該難度小節是否有有效文字為準。
    pub availability_hint: bool,
    /// 難度格子本身的連結（有才記），絕對 URL。
    pub anchor_url: Option<String>,
}

/// Index 中的一首歌。以 `(chart_type, page_id)` 識別，不只用歌名。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiSong {
    pub page_id: u32,
    pub title: String,
    pub chart_type: ChartType,
    pub page_url: String,
    pub difficulties: HashMap<Difficulty, WikiDifficultyInfo>,
    /// Index 表格的分區標題（例如曲風分類），僅供除錯與顯示。
    pub section: Option<String>,
}

/// 歌曲頁中某個難度小節的抽取結果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiChartSection {
    pub difficulty: Difficulty,
    /// 接近 Wiki 顯示內容的原始文字（已保留換行、截到結尾 `E`）。
    /// Wiki 層不解讀 simai 語法，原樣交給既有 simai parser。
    pub raw_text: String,
    /// 是否真的有譜：空白或不含 simai 分割記號即為 false。
    pub available: bool,
}

/// 歌曲頁解析結果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiSongPage {
    pub page_id: u32,
    pub title: String,
    pub artist: Option<String>,
    /// 歌曲資訊表的 BPM（僅 metadata，不取代譜面內的 BPM 事件）。
    pub bpm: Option<f64>,
    pub charts: HashMap<Difficulty, WikiChartSection>,
}

/// Wiki 層錯誤；命令邊界轉成可讀字串（與既有 `majdata.rs` 風格一致）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WikiError {
    Network(String),
    InvalidIndexPage(String),
    InvalidSongPage(String),
    ChartNotFound(Difficulty),
    ChartUnavailable(Difficulty),
    SimaiParseError(String),
    Cache(String),
    InvalidParams(String),
}

impl fmt::Display for WikiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Network(message) => write!(f, "{message}"),
            Self::InvalidIndexPage(message) => write!(f, "{message}"),
            Self::InvalidSongPage(message) => write!(f, "{message}"),
            Self::ChartNotFound(difficulty) => write!(
                f,
                "simai Wiki 的歌曲頁沒有 {} 小節",
                difficulty.section_heading()
            ),
            Self::ChartUnavailable(difficulty) => write!(
                f,
                "simai Wiki 的 {} 小節目前沒有文字譜面",
                difficulty.section_heading()
            ),
            Self::SimaiParseError(message) => write!(f, "{message}"),
            Self::Cache(message) => write!(f, "{message}"),
            Self::InvalidParams(message) => write!(f, "{message}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn difficulty_mappings_cover_index_and_sections() {
        assert_eq!(Difficulty::ALL.len(), 6);
        let cases = [
            (Difficulty::Easy, "ESY", "EASY"),
            (Difficulty::Basic, "BSC", "BASIC"),
            (Difficulty::Advanced, "ADV", "ADVANCED"),
            (Difficulty::Expert, "EXP", "EXPERT"),
            (Difficulty::Master, "MAS", "MASTER"),
            (Difficulty::ReMaster, "Re:MAS", "Re:MASTER"),
        ];
        for (difficulty, header, section) in cases {
            assert_eq!(difficulty.index_header(), header);
            assert_eq!(difficulty.section_heading(), section);
            assert_eq!(Difficulty::from_param(header), Some(difficulty));
            assert_eq!(Difficulty::from_param(section), Some(difficulty));
            assert_eq!(
                Difficulty::from_param(&section.to_ascii_lowercase()),
                Some(difficulty)
            );
        }
        assert_eq!(Difficulty::from_param("dx"), None);
    }

    #[test]
    fn chart_type_params_and_labels() {
        assert_eq!(ChartType::from_param("standard"), Some(ChartType::Standard));
        assert_eq!(ChartType::from_param("STD"), Some(ChartType::Standard));
        assert_eq!(ChartType::from_param("dx"), Some(ChartType::Deluxe));
        assert_eq!(ChartType::from_param("DeLuxe"), Some(ChartType::Deluxe));
        assert_eq!(ChartType::from_param("utage"), None);
        assert!(ChartType::Standard < ChartType::Deluxe);
    }
}
