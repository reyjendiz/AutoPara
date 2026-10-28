//! The rolling transcript and the notes built from it (plan step 7.7 a–d, f).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::llm::Message;
use crate::{ClassContext, Segment, Term};

/// Live notes, grouped as the plan asks (7.7 b, d).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Notes {
    pub topic: String,
    pub key_points: Vec<String>,
    pub definitions: Vec<String>,
    pub deadlines: Vec<String>,
    /// "Питання / Завдання": questions and tasks heard in class, never answered.
    pub flagged: Vec<String>,
}

impl Notes {
    /// Take a notes reply, keeping the flagged items, which come from the transcript itself.
    pub fn apply(&mut self, reply: &NotesReply) {
        self.topic = reply.topic.clone();
        self.key_points = reply.key_points.clone();
        self.definitions = reply.definitions.clone();
        self.deadlines = reply.deadlines.clone();
    }

    /// Markdown for export. `t` looks up the section headings in the string table.
    pub fn to_markdown(&self, t: impl Fn(&str) -> String) -> String {
        let mut out = format!("# {}\n", self.topic);
        let sections = [
            ("assist_md_key_points", &self.key_points),
            ("assist_md_definitions", &self.definitions),
            ("assist_md_deadlines", &self.deadlines),
            ("assist_md_flagged", &self.flagged),
        ];
        for (key, items) in sections.into_iter().filter(|(_, items)| !items.is_empty()) {
            out.push_str(&format!("\n## {}\n\n", t(key)));
            for item in items {
                out.push_str(&format!("- {item}\n"));
            }
        }
        out
    }
}

/// What the notes request asks the model to return.
#[derive(Debug, Default, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct NotesReply {
    pub topic: String,
    pub key_points: Vec<String>,
    pub definitions: Vec<String>,
    pub deadlines: Vec<String>,
    pub terms: Vec<Term>,
    /// The lecture so far, carried into the next request instead of the old transcript.
    pub summary: String,
}

/// The first `{` to the last `}`: local models often wrap JSON in prose or a code fence.
pub fn parse_reply(reply: &str) -> Option<NotesReply> {
    let start = reply.find('{')?;
    let end = reply.rfind('}')?;
    serde_json::from_str(reply.get(start..=end)?).ok()
}

/// Recent transcript sent with each request; with the running summary it bounds the tokens.
const WINDOW_CHARS: usize = 8000;

/// The transcript so far and the model's running summary of it.
#[derive(Debug, Default)]
pub struct Transcript {
    pub segments: Vec<Segment>,
    pub summary: String,
}

impl Transcript {
    pub fn push(&mut self, segment: Segment) {
        self.segments.push(segment);
    }

    pub fn end_ms(&self) -> u64 {
        self.segments.last().map_or(0, |s| s.end_ms)
    }

    /// Text of the segments that end after `ms`, cut to the newest [`WINDOW_CHARS`].
    pub fn since(&self, ms: u64) -> String {
        let text = self
            .segments
            .iter()
            .filter(|s| s.end_ms > ms)
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let skip = text.chars().count().saturating_sub(WINDOW_CHARS);
        text.chars().skip(skip).collect()
    }

    /// The last `minutes` of speech.
    pub fn window(&self, minutes: u32) -> String {
        self.since(self.end_ms().saturating_sub(u64::from(minutes) * 60_000))
    }

    pub fn last_text(&self) -> &str {
        self.segments.last().map_or("", |s| s.text.as_str())
    }
}

/// The periodic notes request (plan 7.7 b–c).
pub fn notes_messages(
    context: &ClassContext,
    transcript: &Transcript,
    window_minutes: u32,
) -> Vec<Message> {
    let system = "You take lecture notes for a student. Reply with one JSON object and nothing \
        else: {\"topic\": string, \"key_points\": [string], \"definitions\": [string], \
        \"deadlines\": [string], \"terms\": [{\"term\": string, \"explanation\": string}], \
        \"summary\": string}. Write in Ukrainian whatever the lecture's language; keep terms and names as said.\
        key_points: at most 8 short bullets. definitions: \"term — definition\" as said in \
        class. deadlines: homework, tasks and dates mentioned. terms: at most 8 new terms or \
        names, each with a one-line explanation. summary: the whole lecture so far in at most \
        5 sentences, extending the previous summary. Use only what was said. Do not answer \
        questions or solve tasks from the lecture.";
    let user = format!(
        "Class: {} ({})\nPrevious summary: {}\nRecent transcript:\n{}",
        context.subject,
        context.teacher,
        transcript.summary,
        transcript.window(window_minutes)
    );
    vec![Message::system(system), Message::user(user)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(start_s: u64, end_s: u64, text: &str) -> Segment {
        Segment {
            start_ms: start_s * 1000,
            end_ms: end_s * 1000,
            text: text.into(),
            flag: None,
        }
    }

    #[test]
    fn window_keeps_the_recent_minutes() {
        let mut transcript = Transcript::default();
        transcript.push(segment(0, 10, "перше"));
        transcript.push(segment(100, 130, "друге"));
        transcript.push(segment(170, 200, "третє"));
        assert_eq!(transcript.window(1), "третє");
        assert_eq!(transcript.window(2), "друге третє");
        assert_eq!(transcript.window(10), "перше друге третє");
        assert_eq!(transcript.since(130_000), "третє");
        assert_eq!(transcript.last_text(), "третє");
        assert_eq!(Transcript::default().window(5), "");
    }

    #[test]
    fn window_is_bounded() {
        let mut transcript = Transcript::default();
        for i in 0..2000 {
            transcript.push(segment(i, i + 1, "слово слово"));
        }
        assert_eq!(transcript.window(60).chars().count(), WINDOW_CHARS);
    }

    #[test]
    fn replies_wrapped_in_prose_parse() {
        let reply = "Ось нотатки:\n```json\n{\"topic\": \"Графи\", \"key_points\": [\"Дерево\"], \
                     \"terms\": [{\"term\": \"Ребро\", \"explanation\": \"зв’язок\"}], \
                     \"summary\": \"Про графи.\"}\n```";
        let parsed = parse_reply(reply).unwrap();
        assert_eq!(parsed.topic, "Графи");
        assert_eq!(parsed.terms[0].term, "Ребро");
        assert!(parsed.deadlines.is_empty());
        assert_eq!(parse_reply("no json here"), None);
    }

    #[test]
    fn notes_keep_flags_and_export_markdown() {
        let mut notes = Notes {
            flagged: vec!["Що таке дерево?".into()],
            ..Notes::default()
        };
        notes.apply(&NotesReply {
            topic: "Графи".into(),
            key_points: vec!["Дерево — граф без циклів".into()],
            ..NotesReply::default()
        });
        assert_eq!(notes.flagged.len(), 1);
        let md = notes.to_markdown(|key| key.to_uppercase());
        assert_eq!(
            md,
            "# Графи\n\n## ASSIST_MD_KEY_POINTS\n\n- Дерево — граф без циклів\n\n\
             ## ASSIST_MD_FLAGGED\n\n- Що таке дерево?\n"
        );
    }

    #[test]
    fn notes_request_carries_class_summary_and_window() {
        let mut transcript = Transcript {
            summary: "Раніше: вступ.".into(),
            ..Transcript::default()
        };
        transcript.push(segment(0, 5, "Сьогодні графи."));
        let context = ClassContext {
            subject: "Дискретна математика".into(),
            ..ClassContext::default()
        };
        let messages = notes_messages(&context, &transcript, 5);
        assert_eq!(messages.len(), 2);
        assert!(messages[0].content.contains("JSON"));
        let user = &messages[1].content;
        assert!(user.contains("Дискретна математика"));
        assert!(user.contains("Раніше: вступ."));
        assert!(user.contains("Сьогодні графи."));
    }
}
