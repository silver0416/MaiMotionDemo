use crate::{geometry, Chart, Diagnostic, Note, SourceSpan};

struct Parser<'a> {
    source: &'a str,
    chars: Vec<(usize, char)>,
    at: usize,
    bpm: Option<f64>,
    division: Option<f64>,
    fixed_step: Option<f64>,
    time: f64,
    chart: Chart,
}
impl<'a> Parser<'a> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).map(|x| x.1)
    }
    fn span(&self, start: usize) -> SourceSpan {
        let begin = self.chars.get(start).map_or(self.source.len(), |c| c.0);
        let end = self.chars.get(self.at).map_or(self.source.len(), |c| c.0);
        let prefix = &self.source[..begin];
        SourceSpan {
            start: begin,
            end,
            line: prefix.chars().filter(|c| *c == '\n').count() + 1,
            column: prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1,
        }
    }
    fn error(&self, code: &str, message: &str, start: usize) -> Diagnostic {
        let mut d = Diagnostic::plain(code, message.into());
        d.source_span = Some(Box::new(self.span(start)));
        d
    }
    fn enclosed(&mut self, open: char, close: char) -> Result<String, Diagnostic> {
        let start = self.at;
        if self.peek() != Some(open) {
            return Err(self.error("invalid", "缺少長度或括號", start));
        }
        self.at += 1;
        let mut s = String::new();
        while let Some(c) = self.peek() {
            self.at += 1;
            if c == close {
                return Ok(s);
            }
            if matches!(c, ',' | 'E') {
                break;
            }
            s.push(c);
        }
        Err(self.error("invalid", "括號沒有結束", start))
    }
    fn positive(&self, s: &str, start: usize) -> Result<f64, Diagnostic> {
        let n = s
            .parse::<f64>()
            .map_err(|_| self.error("invalid", "需要正數", start))?;
        if !n.is_finite() || n <= 0.0 || n > 1_000_000.0 {
            Err(self.error("invalid", "數值超出範圍", start))
        } else {
            Ok(n)
        }
    }
    fn bpm(&self, start: usize) -> Result<f64, Diagnostic> {
        self.bpm
            .ok_or_else(|| self.error("invalid", "請先指定 BPM，例如 (120)", start))
    }
    fn duration(&self, s: &str, start: usize) -> Result<f64, Diagnostic> {
        let result = if let Some(seconds) = s.strip_prefix('#') {
            self.positive(seconds, start)?
        } else {
            let (bpm, rest) = if let Some((b, r)) = s.split_once('#') {
                (self.positive(b, start)?, r)
            } else {
                (self.bpm(start)?, s)
            };
            let (n, m) = rest.split_once(':').ok_or_else(|| {
                self.error(
                    "unsupported",
                    "此持續時間寫法尚未支援，請用 [n:m] 或 [#秒數]",
                    start,
                )
            })?;
            240.0 * self.positive(m, start)? / self.positive(n, start)? / bpm
        };
        if !result.is_finite() || !(0.001..=120.0).contains(&result) {
            return Err(self.error("invalid", "單一長音時間須為 0.001–120 秒", start));
        }
        Ok(result)
    }
    fn note(&mut self) -> Result<(), Diagnostic> {
        let start = self.at;
        let c = self
            .peek()
            .ok_or_else(|| self.error("invalid", "缺少音符", start))?;
        if !('1'..='8').contains(&c) {
            self.at += 1;
            return Err(self.error(
                "unsupported",
                "目前支援外圈 Tap、Hold 與 - ^ < > Slide；此符號尚未支援",
                start,
            ));
        }
        let button = c as u8 - b'0';
        self.at += 1;
        let id = format!("n{}", self.chart.notes.len());
        let mut note = Note {
            id: id.clone(),
            kind: "tap".into(),
            button,
            time_seconds: self.time,
            end_seconds: self.time,
            position: geometry::button(button),
            path_id: None,
            motion_start: None,
            motion_end: None,
            source_span: self.span(start),
        };
        if self.peek() == Some('h') {
            self.at += 1;
            let s = self.enclosed('[', ']')?;
            note.kind = "hold".into();
            note.end_seconds += self.duration(&s, start)?;
        } else if self
            .peek()
            .is_some_and(|c| matches!(c, '-' | '^' | '<' | '>'))
        {
            let shape = self.peek().unwrap();
            self.at += 1;
            let end = self
                .peek()
                .filter(|c| ('1'..='8').contains(c))
                .ok_or_else(|| self.error("invalid", "Slide 缺少 1–8 終點", start))?
                as u8
                - b'0';
            self.at += 1;
            let s = self.enclosed('[', ']')?;
            let (wait, duration) = if let Some((w, d)) = s.split_once("##") {
                let wait = w
                    .parse::<f64>()
                    .map_err(|_| self.error("invalid", "Slide 等待時間錯誤", start))?;
                if !wait.is_finite() || !(0.0..=120.0).contains(&wait) {
                    return Err(self.error("invalid", "Slide 等待時間須為 0–120 秒", start));
                }
                let duration = if d.contains(':') || d.starts_with('#') {
                    self.duration(d, start)?
                } else {
                    self.duration(&format!("#{d}"), start)?
                };
                (wait, duration)
            } else {
                if s.contains('#') {
                    return Err(self.error(
                        "unsupported",
                        "Slide 局部 BPM 時間寫法尚未支援，請用 [n:m] 或 [等待秒##移動秒]",
                        start,
                    ));
                }
                (60.0 / self.bpm(start)?, self.duration(&s, start)?)
            };
            let path_id = format!("p{}", self.chart.paths.len());
            let path = geometry::path(path_id.clone(), button, end, shape)
                .map_err(|m| self.error("invalid", &m, start))?;
            self.chart.paths.push(path);
            note.kind = "slide".into();
            note.path_id = Some(path_id);
            note.motion_start = Some(self.time + wait);
            note.motion_end = Some(self.time + wait + duration);
            note.end_seconds = self.time + wait + duration;
        }
        note.source_span = self.span(start);
        self.chart.duration_seconds = self.chart.duration_seconds.max(note.end_seconds);
        self.chart.notes.push(note);
        if self.chart.notes.len() > 500 {
            return Err(self.error("invalid", "Demo 每次最多分析 500 個音符，請縮短片段", start));
        }
        Ok(())
    }
}

pub fn parse_chart(source: &str, first_seconds: f64) -> Result<Chart, Diagnostic> {
    if source.len() > 100_000
        || !first_seconds.is_finite()
        || !(0.0..=120.0).contains(&first_seconds)
    {
        return Err(Diagnostic::plain(
            "invalid",
            "輸入最多 100 KB；firstSeconds 須為 0–120 秒".into(),
        ));
    }
    let mut p = Parser {
        source,
        chars: source
            .char_indices()
            .filter(|(_, c)| !c.is_whitespace())
            .collect(),
        at: 0,
        bpm: None,
        division: None,
        fixed_step: None,
        time: first_seconds,
        chart: Chart {
            duration_seconds: first_seconds,
            notes: vec![],
            paths: vec![],
        },
    };
    let mut ended = false;
    let mut had_note = false;
    let mut slash = false;
    let mut previous_tap = false;
    while let Some(c) = p.peek() {
        let start = p.at;
        match c {
            '(' if !had_note && !slash => {
                let s = p.enclosed('(', ')')?;
                p.bpm = Some(p.positive(&s, start)?);
            }
            '{' if !had_note && !slash => {
                let s = p.enclosed('{', '}')?;
                if let Some(s) = s.strip_prefix('#') {
                    p.fixed_step = Some(p.positive(s, start)?);
                } else {
                    p.bpm(start)?;
                    p.division = Some(p.positive(&s, start)?);
                    p.fixed_step = None;
                }
            }
            ',' => {
                if slash {
                    return Err(p.error("invalid", "/ 後缺少音符", start));
                }
                let step = match p.fixed_step {
                    Some(s) => s,
                    None => {
                        240.0
                            / p.bpm(start)?
                            / p.division.ok_or_else(|| {
                                p.error("invalid", "請先指定分割，例如 {4}", start)
                            })?
                    }
                };
                p.time += step;
                p.chart.duration_seconds = p.chart.duration_seconds.max(p.time);
                p.at += 1;
                if p.time > 600.0 {
                    return Err(p.error("invalid", "Demo 時間軸上限為 600 秒", start));
                }
                had_note = false;
                previous_tap = false;
            }
            '/' => {
                if !had_note || slash {
                    return Err(p.error("invalid", "/ 必須放在兩個音符之間", start));
                }
                p.at += 1;
                slash = true;
            }
            'E' => {
                p.at += 1;
                if slash || had_note {
                    return Err(p.error("invalid", "E 前需要逗號且不能有未完成的 EACH", start));
                }
                if p.peek().is_some() {
                    return Err(p.error("invalid", "E 後還有內容；TOUCH E 區目前未支援", start));
                }
                ended = true;
                break;
            }
            _ => {
                if had_note && !slash && !previous_tap {
                    return Err(p.error(
                        "unsupported",
                        "複合音符需用 /；連結與修飾語法尚未支援",
                        start,
                    ));
                }
                p.note()?;
                let is_tap = p.chart.notes.last().unwrap().kind == "tap";
                if had_note && !slash && !is_tap {
                    return Err(p.error("invalid", "只有純 Tap 可省略 EACH 的 /", start));
                }
                previous_tap = is_tap;
                had_note = true;
                slash = false;
            }
        }
    }
    if !ended {
        return Err(p.error("invalid", "譜面缺少結束符號 E", p.at));
    }
    if p.chart.notes.is_empty() {
        return Err(p.error("invalid", "譜面沒有音符", 0));
    }
    if p.chart.duration_seconds > 600.0 {
        return Err(p.error("invalid", "Demo 時間軸上限為 600 秒", 0));
    }
    Ok(p.chart)
}
