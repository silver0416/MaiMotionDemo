use crate::geometry::{self, Segment, Shape};
use crate::{Chart, Diagnostic, Modifiers, Note, SourceSpan};

/// 疑似 EACH（`` ` ``）的位移：一小節的 1/384，也就是 1/96 拍。
const PSEUDO_EACH_BEATS: f64 = 1.0 / 96.0;
/// MajdataEdit 的反引號間隔為 128 分音（四分音符的 1/32）。
const MAJDATA_PSEUDO_EACH_BEATS: f64 = 1.0 / 32.0;

const MAX_NOTES: usize = 10_000;
const MAX_SECONDS: f64 = 3600.0;

fn is_shape_start(c: char) -> bool {
    matches!(
        c,
        '-' | '^' | '<' | '>' | 'v' | 'V' | 'p' | 'q' | 's' | 'z' | 'w'
    )
}

fn is_note_start(c: char) -> bool {
    ('1'..='8').contains(&c) || matches!(c, 'A' | 'B' | 'C' | 'D' | 'E')
}

/// 一段有自己長度的 Slide 本體。連續寫法與 `*` 會產生多個本體。
struct Body {
    segments: Vec<Segment>,
    motion_start: f64,
    motion_end: f64,
    modifiers: Modifiers,
}

struct Parser<'a> {
    source: &'a str,
    chars: Vec<(usize, char)>,
    at: usize,
    bpm: Option<f64>,
    division: Option<f64>,
    fixed_step: Option<f64>,
    /// 目前逗號位置的時間。
    time: f64,
    /// 疑似 EACH 在本組內累積的位移數。
    pseudo: u32,
    pseudo_each_beats: f64,
    chart: Chart,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).map(|x| x.1)
    }
    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.at + offset).map(|x| x.1)
    }
    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn span(&self, start: usize) -> SourceSpan {
        self.span_between(start, self.at)
    }
    fn span_between(&self, start: usize, end: usize) -> SourceSpan {
        let begin = self.chars.get(start).map_or(self.source.len(), |c| c.0);
        let end = self.chars.get(end).map_or(self.source.len(), |c| c.0);
        let prefix = &self.source[..begin];
        SourceSpan {
            start: begin,
            end: end.max(begin),
            line: prefix.chars().filter(|c| *c == '\n').count() + 1,
            column: prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1,
        }
    }
    fn error(&self, code: &str, message: &str, start: usize) -> Diagnostic {
        let mut d = Diagnostic::plain(code, message.into());
        d.source_span = Some(Box::new(self.span(start)));
        d
    }
    /// Hold 長度。simai 允許省略 `[...]`（例如 `4h`、`Ch`），與 MajSimai 相同視為長度 0 的 Hold。
    fn hold_length(&mut self, start: usize) -> Result<f64, Diagnostic> {
        if self.peek() != Some('[') {
            return Ok(0.0);
        }
        let spec = self.enclosed('[', ']')?;
        self.duration(&spec, start)
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
    fn beat(&self, start: usize) -> Result<f64, Diagnostic> {
        Ok(60.0 / self.bpm(start)?)
    }
    /// 目前這顆音符的時間，含疑似 EACH 位移。
    fn now(&self, start: usize) -> Result<f64, Diagnostic> {
        if self.pseudo == 0 {
            return Ok(self.time);
        }
        Ok(self.time + self.pseudo as f64 * self.beat(start)? * self.pseudo_each_beats)
    }
    fn digit(&mut self, start: usize, what: &str) -> Result<u8, Diagnostic> {
        let c = self
            .peek()
            .filter(|c| ('1'..='8').contains(c))
            .ok_or_else(|| self.error("invalid", what, start))?;
        self.at += 1;
        Ok(c as u8 - b'0')
    }

    /// `[n:m]`、`[#秒]`、`[bpm#n:m]`。回傳秒數。
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
                self.error("invalid", "長度需寫成 [n:m]、[#秒數] 或 [bpm#n:m]", start)
            })?;
            240.0 * self.positive(m, start)? / self.positive(n, start)? / bpm
        };
        if !result.is_finite() || result <= 0.0 || result > 120.0 {
            return Err(self.error("invalid", "單一長音時間須大於 0 且不超過 120 秒", start));
        }
        Ok(result)
    }

    /// Slide 的長度規格，回傳（等待秒數, 移動秒數）。
    /// `[n:m]`、`[#秒]`、`[bpm#n:m]`、`[等待秒##移動規格]`。
    fn slide_time(&self, s: &str, start: usize) -> Result<(f64, f64), Diagnostic> {
        if let Some((w, d)) = s.split_once("##") {
            let wait = if w.is_empty() {
                self.beat(start)?
            } else {
                let wait = w
                    .parse::<f64>()
                    .map_err(|_| self.error("invalid", "Slide 等待時間錯誤", start))?;
                if !wait.is_finite() || !(0.0..=120.0).contains(&wait) {
                    return Err(self.error("invalid", "Slide 等待時間須為 0–120 秒", start));
                }
                wait
            };
            let duration = if d.contains(':') || d.starts_with('#') {
                self.duration(d, start)?
            } else {
                self.duration(&format!("#{d}"), start)?
            };
            return Ok((wait, duration));
        }
        // [bpm#n:m]：等待與移動都用這個局部 BPM。
        if let Some((b, rest)) = s.split_once('#') {
            if !b.is_empty() {
                let bpm = self.positive(b, start)?;
                let (n, m) = rest
                    .split_once(':')
                    .ok_or_else(|| self.error("invalid", "局部 BPM 長度需寫成 [bpm#n:m]", start))?;
                let duration = 240.0 * self.positive(m, start)? / self.positive(n, start)? / bpm;
                if !duration.is_finite() || duration <= 0.0 || duration > 120.0 {
                    return Err(self.error(
                        "invalid",
                        "單一長音時間須大於 0 且不超過 120 秒",
                        start,
                    ));
                }
                return Ok((60.0 / bpm, duration));
            }
        }
        Ok((self.beat(start)?, self.duration(s, start)?))
    }

    /// 音符或 Slide 本體後面的修飾語。
    fn modifiers(&mut self, m: &mut Modifiers, slide_body: bool) {
        loop {
            match self.peek() {
                Some('b') => {
                    if slide_body {
                        m.break_slide = true;
                    } else {
                        m.break_note = true;
                    }
                }
                Some('x') => {
                    if slide_body {
                        m.ex_slide = true;
                    } else {
                        m.ex = true;
                    }
                }
                Some('f') => m.fireworks = true,
                Some('$') => {
                    if m.star {
                        m.spin_star = true;
                    }
                    m.star = true;
                }
                _ => return,
            }
            self.at += 1;
        }
    }

    fn push(&mut self, note: Note, start: usize) -> Result<(), Diagnostic> {
        self.chart.duration_seconds = self.chart.duration_seconds.max(note.end_seconds);
        self.chart.notes.push(note);
        if self.chart.notes.len() > MAX_NOTES {
            return Err(self.error(
                "invalid",
                "Demo 每次最多分析 10000 個音符，請縮短片段",
                start,
            ));
        }
        Ok(())
    }

    fn blank(&self, kind: &str, button: u8, position: crate::Point, time: f64) -> Note {
        Note {
            id: format!("n{}", self.chart.notes.len()),
            key: String::new(),
            kind: kind.into(),
            button,
            touch_area: None,
            time_seconds: time,
            end_seconds: time,
            position,
            path_id: None,
            motion_start: None,
            motion_end: None,
            has_head: true,
            modifiers: Modifiers::default(),
            source_span: self.span(self.at),
        }
    }

    /// Touch：`A1`–`E8`、`C`、`C1`、`C2`，以及 Touch Hold `Ch[4:1]`。
    fn touch_note(&mut self, start: usize) -> Result<(), Diagnostic> {
        let area = self.peek().unwrap();
        self.at += 1;
        let index = if area == 'C' {
            if matches!(self.peek(), Some('1') | Some('2')) {
                self.at += 1;
            }
            0
        } else {
            self.digit(start, "Touch 區域需要 1–8 的編號")?
        };
        let time = self.now(start)?;
        let mut note = self.blank("touch", index, geometry::touch(area, index), time);
        note.touch_area = Some(area.to_string());
        self.modifiers(&mut note.modifiers, false);
        if self.eat('h') {
            self.modifiers(&mut note.modifiers, false);
            note.kind = "touchHold".into();
            note.end_seconds = time + self.hold_length(start)?;
            self.modifiers(&mut note.modifiers, false);
        }
        note.source_span = self.span(start);
        self.push(note, start)
    }

    /// 形狀符號與終點鍵；`V` 另外讀轉折鍵。
    fn shape(&mut self, start: usize) -> Result<(Shape, u8), Diagnostic> {
        let c = self
            .peek()
            .ok_or_else(|| self.error("invalid", "Slide 缺少形狀", start))?;
        self.at += 1;
        let shape = match c {
            '-' => Shape::Line,
            '^' | '<' | '>' => Shape::Arc(c),
            'v' => Shape::Center,
            'V' => {
                let turn = self.digit(start, "V 形 Slide 需要轉折鍵 1–8")?;
                Shape::Grand(turn)
            }
            'p' | 'q' => {
                let wide = self.eat(c);
                Shape::Loop {
                    ccw: c == 'p',
                    wide,
                }
            }
            's' => Shape::S,
            'z' => Shape::Z,
            'w' => Shape::Wifi,
            _ => return Err(self.error("invalid", "不認得的 Slide 形狀", start)),
        };
        let end = self.digit(start, "Slide 缺少 1–8 終點")?;
        Ok((shape, end))
    }

    fn slide(
        &mut self,
        head: u8,
        head_modifiers: Modifiers,
        start: usize,
    ) -> Result<(), Diagnostic> {
        let no_head = self.eat('?') || self.eat('!');
        let head_time = self.now(start)?;
        let mut bodies: Vec<Body> = vec![];
        let mut cursor = head;
        let mut chained_from: Option<f64> = None;
        loop {
            let mut segments = vec![];
            loop {
                let (shape, end) = self.shape(start)?;
                segments.push(Segment {
                    start: cursor,
                    end,
                    shape,
                });
                cursor = end;
                if self.peek() == Some('[')
                    || (self.peek() == Some('b') && self.peek_at(1) == Some('['))
                {
                    break;
                }
                if !self.peek().is_some_and(is_shape_start) {
                    return Err(self.error("invalid", "Slide 缺少長度，例如 [4:1]", start));
                }
            }
            let mut modifiers = Modifiers::default();
            // Majdata 譜例允許把 Break Slide 的 b 放在長度括號之前。
            if self.eat('b') {
                modifiers.break_slide = true;
            }
            let spec = self.enclosed('[', ']')?;
            let (wait, duration) = self.slide_time(&spec, start)?;
            self.modifiers(&mut modifiers, true);
            let motion_start = chained_from.unwrap_or(head_time + wait);
            bodies.push(Body {
                segments,
                motion_start,
                motion_end: motion_start + duration,
                modifiers,
            });
            if self.eat('*') {
                // 同一個起點同時發出的第二條以上 Slide。
                cursor = head;
                chained_from = None;
                continue;
            }
            if self.peek().is_some_and(is_shape_start) {
                chained_from = Some(bodies[bodies.len() - 1].motion_end);
                continue;
            }
            break;
        }
        let end_at = self.at;
        for (index, body) in bodies.into_iter().enumerate() {
            let path_id = format!("p{}", self.chart.paths.len());
            let path = geometry::build_path(path_id.clone(), &body.segments)
                .map_err(|m| self.error("invalid", &m, start))?;
            self.chart.paths.push(path);
            let has_head = index == 0 && !no_head;
            let time = if has_head {
                head_time
            } else {
                body.motion_start
            };
            let mut note = self.blank("slide", head, geometry::button(head), time);
            note.path_id = Some(path_id);
            note.motion_start = Some(body.motion_start);
            note.motion_end = Some(body.motion_end);
            note.end_seconds = body.motion_end;
            note.has_head = has_head;
            note.modifiers = Modifiers {
                break_slide: body.modifiers.break_slide,
                ex_slide: body.modifiers.ex_slide,
                ..head_modifiers
            };
            note.source_span = self.span_between(start, end_at);
            self.push(note, start)?;
        }
        Ok(())
    }

    fn note(&mut self) -> Result<(), Diagnostic> {
        let start = self.at;
        let c = self
            .peek()
            .ok_or_else(|| self.error("invalid", "缺少音符", start))?;
        if matches!(c, 'A' | 'B' | 'C' | 'D' | 'E') {
            return self.touch_note(start);
        }
        if !('1'..='8').contains(&c) {
            self.at += 1;
            return Err(self.error("invalid", "音符必須是 1–8 或 Touch 區 A–E", start));
        }
        let button = c as u8 - b'0';
        self.at += 1;
        let mut modifiers = Modifiers::default();
        self.modifiers(&mut modifiers, false);
        if self.eat('h') {
            self.modifiers(&mut modifiers, false);
            let time = self.now(start)?;
            let mut note = self.blank("hold", button, geometry::button(button), time);
            note.end_seconds = time + self.hold_length(start)?;
            self.modifiers(&mut modifiers, false);
            note.modifiers = modifiers;
            note.source_span = self.span(start);
            return self.push(note, start);
        }
        if self
            .peek()
            .is_some_and(|c| is_shape_start(c) || c == '?' || c == '!')
        {
            return self.slide(button, modifiers, start);
        }
        let time = self.now(start)?;
        let mut note = self.blank("tap", button, geometry::button(button), time);
        note.modifiers = modifiers;
        note.source_span = self.span(start);
        self.push(note, start)
    }
}

/// maidata.txt 的 `&key=value` 區塊，value 以 byte 範圍表示。
fn maidata_fields(source: &str) -> Vec<(String, usize, usize)> {
    let mut fields: Vec<(String, usize, usize)> = vec![];
    let mut offset = 0usize;
    for line in source.split_inclusive('\n') {
        if let Some(rest) = line.strip_prefix('&') {
            if let Some(eq) = rest.find('=') {
                let key = rest[..eq].trim().to_ascii_lowercase();
                let start = offset + 1 + eq + 1;
                fields.push((key, start, start));
            }
        }
        if let Some(last) = fields.last_mut() {
            last.2 = offset + line.len();
        }
        offset += line.len();
    }
    fields
}

/// 若原文是完整的 maidata.txt，挑出難度編號最大的非空 `&inote_n` 當作譜面本文。
/// 回傳該區塊的 byte 範圍與說明訊息；一般片段回傳整段原文。
fn select_body(source: &str) -> (usize, usize, Vec<Diagnostic>) {
    let fields = maidata_fields(source);
    if fields.is_empty() {
        return (0, source.len(), vec![]);
    }
    let mut notices = vec![];
    let mut charts: Vec<(u32, usize, usize)> = fields
        .iter()
        .filter_map(|(key, start, end)| {
            let level = key.strip_prefix("inote_")?.parse::<u32>().ok()?;
            if source[*start..*end].trim().is_empty() {
                None
            } else {
                Some((level, *start, *end))
            }
        })
        .collect();
    charts.sort_by_key(|c| c.0);
    if let Some((_, start, end)) = fields.iter().find(|(k, ..)| k == "first") {
        let value = source[*start..*end].trim();
        if !value.is_empty() {
            notices.push(Diagnostic::info(
                "maidata_first",
                format!("檔案的 &first= 為 {value} 秒；本 Demo 的起始秒數請在「參數」分頁設定。"),
            ));
        }
    }
    match charts.last() {
        Some((level, start, end)) => {
            notices.push(Diagnostic::info(
                "maidata_chart",
                format!(
                    "讀到 maidata 檔案，已使用 &inote_{level}（共 {} 個難度）。",
                    charts.len()
                ),
            ));
            (*start, *end, notices)
        }
        None => (0, source.len(), notices),
    }
}

/// 去掉空白與 `||` 行註解，保留原文位移供錯誤定位。
fn scan(source: &str) -> Vec<(usize, char)> {
    let mut out = vec![];
    let mut iter = source.char_indices().peekable();
    while let Some((i, c)) = iter.next() {
        if c == '|' && iter.peek().is_some_and(|(_, n)| *n == '|') {
            for (_, c) in iter.by_ref() {
                if c == '\n' {
                    break;
                }
            }
            continue;
        }
        if !c.is_whitespace() {
            out.push((i, c));
        }
    }
    out
}

/// 解析結果：譜面與非錯誤說明（例如從 maidata 選了哪個難度）。
#[derive(Clone, Debug)]
pub struct ParseOutput {
    pub chart: Chart,
    pub notices: Vec<Diagnostic>,
}

pub fn parse_chart(source: &str, first_seconds: f64) -> Result<ParseOutput, Diagnostic> {
    if source.len() > 4_000_000
        || !first_seconds.is_finite()
        || !(0.0..=120.0).contains(&first_seconds)
    {
        return Err(Diagnostic::plain(
            "invalid",
            "輸入最多 4 MB；firstSeconds 須為 0–120 秒".into(),
        ));
    }
    let (body_start, body_end, mut notices) = select_body(source);
    let chars: Vec<(usize, char)> = scan(&source[body_start..body_end])
        .into_iter()
        .map(|(i, c)| (i + body_start, c))
        .collect();
    let majdata_mode = body_start != 0
        || chars
            .windows(4)
            .any(|w| w.iter().map(|(_, c)| *c).eq(['<', 'H', 'S', '*']));
    let mut p = Parser {
        source,
        chars,
        at: 0,
        bpm: None,
        division: None,
        fixed_step: None,
        time: first_seconds,
        pseudo: 0,
        pseudo_each_beats: if majdata_mode {
            MAJDATA_PSEUDO_EACH_BEATS
        } else {
            PSEUDO_EACH_BEATS
        },
        chart: Chart {
            duration_seconds: first_seconds,
            notes: vec![],
            paths: vec![],
            touch_sensors: geometry::touch_sensors(),
        },
    };
    let mut ended = false;
    let mut had_note = false;
    let mut slash = false;
    let mut pseudo_pending = false;
    let mut hs_count = 0usize;
    let mut pseudo_count = 0usize;
    while let Some(c) = p.peek() {
        let start = p.at;
        match c {
            '(' => {
                let s = p.enclosed('(', ')')?;
                p.bpm = Some(p.positive(&s, start)?);
            }
            '{' => {
                let s = p.enclosed('{', '}')?;
                if let Some(s) = s.strip_prefix('#') {
                    p.fixed_step = Some(p.positive(s, start)?);
                } else {
                    p.division = Some(p.positive(&s, start)?);
                    p.fixed_step = None;
                }
            }
            '<' if p.peek_at(1) == Some('H') => {
                p.at += 1;
                if !(p.eat('H') && p.eat('S') && p.eat('*')) {
                    return Err(p.error("invalid", "Majdata 速度指令須寫成 <HS*數字>", start));
                }
                // 已讀過 '*'；只收集直到 '>'，避免把 HS 當成譜面音符。
                let mut value = String::new();
                while let Some(c) = p.peek() {
                    if c == '>' {
                        break;
                    }
                    if c == ',' {
                        return Err(p.error("invalid", "Majdata 速度指令缺少 >", start));
                    }
                    value.push(c);
                    p.at += 1;
                }
                if !p.eat('>') {
                    return Err(p.error("invalid", "Majdata 速度指令缺少 >", start));
                }
                let speed = value
                    .parse::<f64>()
                    .map_err(|_| p.error("invalid", "Majdata HS 速度需要數字", start))?;
                if !speed.is_finite() || speed.abs() > 1_000_000.0 {
                    return Err(p.error("invalid", "Majdata HS 速度超出範圍", start));
                }
                hs_count += 1;
            }
            ',' => {
                if slash {
                    return Err(p.error("invalid", "/ 後缺少音符", start));
                }
                if pseudo_pending {
                    return Err(p.error("invalid", "反引號後缺少音符", start));
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
                if p.time > MAX_SECONDS {
                    return Err(p.error("invalid", "時間軸上限為 3600 秒", start));
                }
                had_note = false;
                slash = false;
                pseudo_pending = false;
                p.pseudo = 0;
            }
            '/' => {
                if !had_note || slash {
                    return Err(p.error("invalid", "/ 必須放在兩個音符之間", start));
                }
                p.at += 1;
                slash = true;
            }
            '`' => {
                if !had_note {
                    return Err(p.error("invalid", "` 必須放在兩個音符之間", start));
                }
                p.at += 1;
                p.pseudo += 1;
                pseudo_count += 1;
                slash = false;
                pseudo_pending = true;
            }
            'E' if !p.peek_at(1).is_some_and(|c| ('1'..='8').contains(&c)) => {
                p.at += 1;
                if slash {
                    return Err(p.error("invalid", "E 前有未完成的 EACH", start));
                }
                if pseudo_pending {
                    return Err(p.error("invalid", "反引號後缺少音符", start));
                }
                if p.peek().is_some() {
                    return Err(p.error("invalid", "E 之後不可再有內容", start));
                }
                ended = true;
                break;
            }
            _ if is_note_start(c) => {
                p.note()?;
                had_note = true;
                slash = false;
                pseudo_pending = false;
            }
            _ => {
                p.at += 1;
                return Err(p.error("invalid", "不認得的 simai 符號", start));
            }
        }
    }
    if !ended && !majdata_mode {
        return Err(p.error("invalid", "譜面缺少結束符號 E", p.at));
    }
    if slash {
        return Err(p.error("invalid", "/ 後缺少音符", p.at));
    }
    if pseudo_pending {
        return Err(p.error("invalid", "反引號後缺少音符", p.at));
    }
    if !ended {
        notices.push(Diagnostic::info(
            "majdata_end",
            "Majdata 譜面未寫 E，已在檔案結尾完成解析。".into(),
        ));
    }
    if hs_count > 0 {
        notices.push(Diagnostic::info(
            "majdata_hs",
            format!("讀到 {hs_count} 個 Majdata HS 顯示速度指令；動作時間仍由 BPM 與分割決定。"),
        ));
    }
    if majdata_mode && pseudo_count > 0 {
        notices.push(Diagnostic::info(
            "majdata_pseudo_each",
            "Majdata 譜面中的反引號已按 128 分音間隔換算。".into(),
        ));
    }
    if p.chart.notes.is_empty() {
        return Err(p.error("invalid", "譜面沒有音符", 0));
    }
    if p.chart.duration_seconds > MAX_SECONDS {
        return Err(p.error("invalid", "時間軸上限為 3600 秒", 0));
    }
    crate::annotation::assign_keys(&mut p.chart);
    Ok(ParseOutput {
        chart: p.chart,
        notices,
    })
}
