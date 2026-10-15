//! ゲームの標準出力の log4j XML を、通常の行に変換する。

use quick_xml::Reader;
use quick_xml::events::Event;
use serde::Serialize;
use ts_rs::TS;

/// 1 件のイベントのバッファの上限。超えたらそのままの文字列として流す。
pub const MAX_EVENT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LogRecord {
    pub time_ms: Option<i64>,
    pub level: Option<String>,
    pub thread: Option<String>,
    pub logger: Option<String>,
    pub message: String,
    pub throwable: Option<String>,
}

impl LogRecord {
    pub fn raw(line: &str) -> Self {
        Self {
            message: line.to_string(),
            ..Self::default()
        }
    }

    /// ファイルに書く形。時刻は UTC で表す（タイムゾーンの取得に依存しないため）。
    pub fn to_file_line(&self) -> String {
        let mut line = match (&self.time_ms, &self.level) {
            (Some(ms), Some(level)) => {
                let s = ms.div_euclid(1000).rem_euclid(86_400);
                format!(
                    "[{:02}:{:02}:{:02} UTC] [{}/{}]: {}",
                    s / 3600,
                    s % 3600 / 60,
                    s % 60,
                    self.thread.as_deref().unwrap_or("?"),
                    level,
                    self.message
                )
            }
            _ => self.message.clone(),
        };
        if let Some(t) = &self.throwable {
            line.push('\n');
            line.push_str(t);
        }
        line
    }
}

/// 8 文字以上の秘密の値だけを伏せる（"0" のような短い値で、普通の文字を伏せないため）。
pub fn redact(text: &str, secret: Option<&str>) -> String {
    match secret {
        Some(s) if s.len() >= 8 && text.contains(s) => text.replace(s, "[REDACTED]"),
        _ => text.to_string(),
    }
}

#[derive(Clone, Copy)]
enum Field {
    Message,
    Throwable,
}

fn append(record: &mut LogRecord, field: Option<Field>, text: &str) {
    match field {
        Some(Field::Message) => record.message.push_str(text),
        Some(Field::Throwable) => record
            .throwable
            .get_or_insert_with(String::new)
            .push_str(text),
        None => {}
    }
}

/// 1 件の `<log4j:Event>` を読む。XML として読めなければ None。
/// quick-xml は DTD の実体を展開しないため、外部実体の読み込みは起こらない。
fn parse_event(text: &str) -> Option<LogRecord> {
    let mut reader = Reader::from_str(text);
    reader.config_mut().expand_empty_elements = true;
    reader.config_mut().check_end_names = true;
    reader.config_mut().check_comments = true;
    let mut record = LogRecord::default();
    let mut field = None;
    let mut depth: usize = 0;
    let mut closed = false;
    loop {
        match reader.read_event().ok()? {
            Event::Start(e) => {
                match (depth, e.name().as_ref()) {
                    (0, "log4j:Event") if !closed => {
                        for attr in e.attributes() {
                            let attr = attr.ok()?;
                            let value = attr
                                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                .ok()?
                                .into_owned();
                            match attr.key.as_ref() {
                                "logger" => record.logger = Some(value),
                                "timestamp" => record.time_ms = value.parse().ok(),
                                "level" => record.level = Some(value),
                                "thread" => record.thread = Some(value),
                                _ => {}
                            }
                        }
                    }
                    (1, "log4j:Message") => field = Some(Field::Message),
                    (1, "log4j:Throwable") => {
                        field = Some(Field::Throwable);
                        record.throwable.get_or_insert_with(String::new);
                    }
                    _ => return None,
                }
                depth += 1;
            }
            Event::End(_) => {
                depth = depth.checked_sub(1)?;
                field = None;
                if depth == 0 {
                    closed = true;
                }
            }
            Event::CData(c) => append(&mut record, field, c.as_ref()),
            Event::Text(t) => {
                if depth == 0 && !t.trim().is_empty() {
                    return None;
                }
                append(&mut record, field, t.as_ref());
            }
            Event::GeneralRef(r) => {
                if let Some(ch) = r.resolve_char_ref().ok()? {
                    append(&mut record, field, &ch.to_string());
                } else {
                    let ch = match r.as_ref() {
                        "lt" => "<",
                        "gt" => ">",
                        "amp" => "&",
                        "quot" => "\"",
                        "apos" => "'",
                        _ => return None,
                    };
                    append(&mut record, field, ch);
                }
            }
            Event::DocType(_) => return None,
            Event::Eof => break,
            _ => {}
        }
    }
    (closed && depth == 0).then_some(record)
}

/// CDATA 内の文字列をイベントの終了と取り違えないよう、XML の区切りを読む。
fn event_end(text: &str) -> Option<usize> {
    let mut reader = Reader::from_str(text);
    reader.config_mut().check_end_names = false;
    loop {
        match reader.read_event() {
            Ok(Event::End(e)) if e.name().as_ref() == "log4j:Event" => {
                return Some(reader.buffer_position() as usize);
            }
            Ok(Event::Eof) => return None,
            Err(_) => return None,
            _ => {}
        }
    }
}

/// 行を受け取り、イベントがそろったら LogRecord を返す。
#[derive(Default)]
pub struct Log4jParser {
    buf: String,
}

impl Log4jParser {
    pub fn new() -> Self {
        Self::default()
    }

    fn flush_raw(&mut self) -> Vec<LogRecord> {
        std::mem::take(&mut self.buf)
            .lines()
            .map(LogRecord::raw)
            .collect()
    }

    pub fn push_line(&mut self, line: &str) -> Vec<LogRecord> {
        if self.buf.is_empty() && !line.trim_start().starts_with("<log4j:Event") {
            return vec![LogRecord::raw(line)];
        }
        let separator = usize::from(!self.buf.is_empty());
        if self
            .buf
            .len()
            .saturating_add(separator)
            .saturating_add(line.len())
            > MAX_EVENT_BYTES
        {
            let mut out = self.flush_raw();
            out.extend(line.lines().map(LogRecord::raw));
            return out;
        }
        if separator != 0 {
            self.buf.push('\n');
        }
        self.buf.push_str(line);
        let mut out = Vec::new();
        while let Some(end) = event_end(&self.buf) {
            let rest = self.buf.split_off(end);
            let text = std::mem::replace(&mut self.buf, rest);
            match parse_event(&text) {
                Some(record) => out.push(record),
                None => out.extend(text.lines().map(LogRecord::raw)),
            }
            if self.buf.trim().is_empty() {
                self.buf.clear();
                break;
            }
            if !self.buf.trim_start().starts_with("<log4j:Event") {
                out.extend(self.flush_raw());
                break;
            }
        }
        out
    }

    /// 終わっていないイベントを、そのままの文字列として返す。
    pub fn finish(&mut self) -> Vec<LogRecord> {
        self.flush_raw()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(lines: &[&str]) -> Vec<LogRecord> {
        let mut p = Log4jParser::new();
        let mut out: Vec<LogRecord> = lines.iter().flat_map(|l| p.push_line(l)).collect();
        out.extend(p.finish());
        out
    }

    #[test]
    fn plain_lines_pass_through() {
        let out = feed(&["Picked up _JAVA_OPTIONS", "hello"]);
        assert_eq!(
            out,
            vec![
                LogRecord::raw("Picked up _JAVA_OPTIONS"),
                LogRecord::raw("hello")
            ]
        );
    }

    #[test]
    fn parses_a_multi_line_event() {
        let out = feed(&[
            r#"<log4j:Event logger="net.minecraft.client.Minecraft" timestamp="1700000000123" level="INFO" thread="Render thread">"#,
            r#"  <log4j:Message><![CDATA[Setting user: Player]]></log4j:Message>"#,
            r#"</log4j:Event>"#,
        ]);
        assert_eq!(out.len(), 1);
        let r = &out[0];
        assert_eq!(r.message, "Setting user: Player");
        assert_eq!(r.level.as_deref(), Some("INFO"));
        assert_eq!(r.thread.as_deref(), Some("Render thread"));
        assert_eq!(r.logger.as_deref(), Some("net.minecraft.client.Minecraft"));
        assert_eq!(r.time_ms, Some(1_700_000_000_123));
        assert_eq!(r.throwable, None);
    }

    #[test]
    fn parses_throwable_and_escaped_text() {
        let out = feed(&[
            r#"<log4j:Event logger="a" timestamp="0" level="ERROR" thread="main"><log4j:Message>a &lt; b &amp; c</log4j:Message><log4j:Throwable><![CDATA[java.lang.Exception: boom]]></log4j:Throwable></log4j:Event>"#,
        ]);
        assert_eq!(out[0].message, "a < b & c");
        assert_eq!(
            out[0].throwable.as_deref(),
            Some("java.lang.Exception: boom")
        );
    }

    #[test]
    fn unterminated_event_is_flushed_raw_at_the_end() {
        let out = feed(&[
            r#"<log4j:Event logger="a" level="INFO">"#,
            "  <log4j:Message><![CDATA[cut",
        ]);
        assert_eq!(out.len(), 2);
        assert_eq!(out[1].message, "  <log4j:Message><![CDATA[cut");
    }

    #[test]
    fn broken_xml_is_passed_through_raw() {
        let out = feed(&[r#"<log4j:Event level="INFO"><log4j:Message><<<</log4j:Event>"#]);
        assert_eq!(out.len(), 1);
        assert!(out[0].message.contains("<<<"));
        assert_eq!(out[0].level, None);
    }

    #[test]
    fn oversized_event_is_flushed_raw() {
        let mut p = Log4jParser::new();
        assert!(p.push_line(r#"<log4j:Event level="INFO">"#).is_empty());
        let big = "x".repeat(MAX_EVENT_BYTES);
        let out = p.push_line(&big);
        assert_eq!(out.len(), 2);
        assert!(p.finish().is_empty());
    }

    #[test]
    fn redaction_hides_long_secrets_only() {
        assert_eq!(
            redact("token abcdefgh123 end", Some("abcdefgh123")),
            "token [REDACTED] end"
        );
        assert_eq!(redact("value 0 here", Some("0")), "value 0 here");
        assert_eq!(redact("nothing", None), "nothing");
    }

    #[test]
    fn file_lines() {
        let r = LogRecord {
            time_ms: Some(3_723_000),
            level: Some("WARN".into()),
            thread: Some("main".into()),
            logger: None,
            message: "msg".into(),
            throwable: Some("trace".into()),
        };
        assert_eq!(r.to_file_line(), "[01:02:03 UTC] [main/WARN]: msg\ntrace");
        assert_eq!(LogRecord::raw("plain").to_file_line(), "plain");
    }
}

#[cfg(test)]
mod edge_tests {
    use super::*;

    #[test]
    fn complete_oversized_event_stays_raw() {
        let text = format!(
            "<log4j:Event><log4j:Message>{}</log4j:Message></log4j:Event>",
            "x".repeat(MAX_EVENT_BYTES)
        );
        let mut p = Log4jParser::new();
        assert!(p.push_line(&text) == vec![LogRecord::raw(&text)]);
        assert!(p.buf.len() <= MAX_EVENT_BYTES);
    }

    #[test]
    fn adjacent_events_are_separate_records() {
        let event = "<log4j:Event><log4j:Message>hello</log4j:Message></log4j:Event>";
        let mut p = Log4jParser::new();
        assert_eq!(
            p.push_line(&event.repeat(2))
                .iter()
                .map(|r| r.message.as_str())
                .collect::<Vec<_>>(),
            vec!["hello", "hello"]
        );
    }

    #[test]
    fn numeric_references_are_decoded_and_unknown_entities_stay_raw() {
        let mut p = Log4jParser::new();
        let text = "<log4j:Event><log4j:Message>&#65;&#x42;</log4j:Message></log4j:Event>";
        assert_eq!(p.push_line(text)[0].message, "AB");
        let text = text.replace("&#65;&#x42;", "&external;");
        assert_eq!(p.push_line(&text), vec![LogRecord::raw(&text)]);
    }

    #[test]
    fn dtd_is_rejected_without_expanding_entities() {
        let text = "<log4j:Event><!DOCTYPE x [<!ENTITY external SYSTEM 'file:///missing'>]><log4j:Message>&external;</log4j:Message></log4j:Event>";
        assert_eq!(
            Log4jParser::new().push_line(text),
            vec![LogRecord::raw(text)]
        );
    }

    #[test]
    fn lossy_utf8_text_passes_through() {
        let text = String::from_utf8_lossy(b"hello\xff");
        assert_eq!(
            Log4jParser::new().push_line(&text),
            vec![LogRecord::raw(&text)]
        );
    }
}

#[cfg(test)]
mod framing_tests {
    use super::*;

    #[test]
    fn closing_tag_inside_cdata_does_not_finish_the_event() {
        let mut p = Log4jParser::new();
        assert!(
            p.push_line("<log4j:Event><log4j:Message><![CDATA[literal </log4j:Event>")
                .is_empty()
        );
        let out = p.push_line("]]></log4j:Message></log4j:Event>");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].message, "literal </log4j:Event>\n");
    }
}
