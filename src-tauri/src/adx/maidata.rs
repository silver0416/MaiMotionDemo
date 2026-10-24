//! adxdls 的譜面是完整 maidata.txt（`&inote_2…6` 各一個難度）。
//! 匯入時只留選定的難度與基本資訊：核心 parser 遇到 maidata 會挑編號最大的 `&inote_n`，
//! 只留一個難度才不會讀錯；同一首的其他難度是另一筆紀錄，和 simai Wiki 匯入一致。

/// `&key=value` 欄位；value 延續到下一個以 `&` 開頭的行之前。key 轉小寫。
pub fn fields(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix('&') {
            if let Some((key, value)) = rest.split_once('=') {
                out.push((key.trim().to_ascii_lowercase(), value.to_string()));
                continue;
            }
        }
        if let Some((_, value)) = out.last_mut() {
            value.push('\n');
            value.push_str(line);
        }
    }
    out
}

pub fn field<'a>(fields: &'a [(String, String)], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.trim())
        .filter(|value| !value.is_empty())
}

/// 取出 `&inote_{slot}` 並組成只含這個難度的 maidata。沒有這個難度或內容空白時回傳 None。
pub fn single_chart(text: &str, slot: u32) -> Option<String> {
    let fields = fields(text.strip_prefix('\u{feff}').unwrap_or(text));
    let body = field(&fields, &format!("inote_{slot}"))?;
    let mut out = String::new();
    for key in ["title", "artist", "wholebpm", "first"] {
        if let Some(value) = field(&fields, key) {
            out.push_str(&format!("&{key}={value}\n"));
        }
    }
    for key in [format!("des_{slot}"), format!("lv_{slot}")] {
        if let Some(value) = field(&fields, &key) {
            out.push_str(&format!("&{key}={value}\n"));
        }
    }
    out.push_str(&format!("&inote_{slot}={body}\n"));
    Some(out)
}

/// `&inote_{slot}=` 所在的行號（從 1 起算）；用來把解析錯誤的位置換回原始 maidata.txt。
pub fn inote_line(text: &str, slot: u32) -> Option<usize> {
    let prefix = format!("&inote_{slot}=");
    text.lines()
        .position(|line| line.to_ascii_lowercase().starts_with(&prefix))
        .map(|index| index + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\u{feff}&title=スイートマジック\n&artist=ろん×Junky\n&first=0\n&wholebpm=123\n&shortid=121\n\n\n&lv_2=4.0\n&des_2=-\n&inote_2=(123){1},,\n{2}4,4,\nE\n\n&lv_5=12.4\n&des_5=ニャイン\n&inote_5=(123){1},\n1,2,\nE\n";

    #[test]
    fn keeps_only_the_chosen_difficulty() {
        let chart = single_chart(SAMPLE, 5).unwrap();
        assert_eq!(
            chart,
            "&title=スイートマジック\n&artist=ろん×Junky\n&wholebpm=123\n&first=0\n&des_5=ニャイン\n&lv_5=12.4\n&inote_5=(123){1},\n1,2,\nE\n"
        );
        assert!(!chart.contains("inote_2"));
        assert!(single_chart(SAMPLE, 2)
            .unwrap()
            .contains("&inote_2=(123){1},,\n{2}4,4,\nE\n"));
    }

    #[test]
    fn finds_inote_line_in_both_texts() {
        assert_eq!(inote_line(SAMPLE, 5), Some(16));
        assert_eq!(inote_line(&single_chart(SAMPLE, 5).unwrap(), 5), Some(7));
        assert_eq!(inote_line(SAMPLE, 3), None);
    }

    #[test]
    fn missing_or_empty_difficulty_is_none() {
        assert!(single_chart(SAMPLE, 6).is_none());
        assert!(single_chart("&title=x\n&inote_4=\n\n", 4).is_none());
    }

    #[test]
    fn single_chart_parses_with_core() {
        let chart = single_chart(SAMPLE, 5).unwrap();
        let parsed = mai_motion_core::parse_chart(&chart, 0.0).unwrap();
        assert_eq!(parsed.chart.notes.len(), 2);
    }
}
