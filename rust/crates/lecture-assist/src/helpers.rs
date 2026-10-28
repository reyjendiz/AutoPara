//! Prompts for the on-demand helpers and the end-of-class summary (plan step 7.7 e, f).

use crate::llm::Message;
use crate::notes::{Notes, Transcript};
use crate::{AskKind, ClassContext};

/// Explaining is the point; doing the student's work, or answering the teacher for them, is not.
const SYSTEM: &str = "You help a student follow a lecture. Answer in Ukrainian whatever the \
    lecture's language. Be brief and use plain text. Explain and summarise only: \
    never write answers to questions the teacher asks the class, and never do tasks or homework \
    for the student.";

/// The request for one helper. `text` is the user's question, or the term to simplify (empty
/// means the last phrase).
pub fn helper_messages(
    kind: AskKind,
    context: &ClassContext,
    transcript: &Transcript,
    notes: &Notes,
    text: &str,
) -> Vec<Message> {
    let class = format!("Class: {} ({})", context.subject, context.teacher);
    let summary = &transcript.summary;
    let user = match kind {
        AskKind::Recap => format!(
            "{class}\nSummarise the last two minutes in 3 to 5 bullets.\n\n{}",
            transcript.window(2)
        ),
        AskKind::Simplify => {
            let target = if text.trim().is_empty() {
                transcript.last_text()
            } else {
                text
            };
            format!(
                "{class}\nExplain this more simply, in 2 or 3 sentences: {target}\n\n\
                 What was said around it:\n{}",
                transcript.window(2)
            )
        }
        AskKind::Topic => format!(
            "{class}\nIn 3 or 4 sentences, what is this class about?\n\nSummary so far: \
             {summary}\nTopic: {}\nRecent:\n{}",
            notes.topic,
            transcript.window(3)
        ),
        AskKind::Free => format!(
            "{class}\nThe student asks about the lecture: {text}\n\nSummary so far: \
             {summary}\nRecent:\n{}",
            transcript.window(5)
        ),
        AskKind::Summary => format!(
            "{class}\nWrite the end-of-class summary in Markdown with sections for the topic, \
             key points, definitions, deadlines and homework, and the questions or tasks heard \
             in class (list them, do not answer them).\n\nSummary so far: {summary}\n\
             Notes: {}\nThe last part of the lecture:\n{}",
            serde_json::to_string(notes).unwrap_or_default(),
            transcript.window(10)
        ),
    };
    vec![Message::system(SYSTEM), Message::user(user)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Segment;

    fn transcript() -> Transcript {
        let mut transcript = Transcript {
            summary: "Вступ до графів.".into(),
            ..Transcript::default()
        };
        for (i, text) in ["Граф — це вершини і ребра.", "Хто знає, що таке дерево?"]
            .iter()
            .enumerate()
        {
            let start = i as u64 * 60_000;
            transcript.push(Segment {
                start_ms: start,
                end_ms: start + 5_000,
                text: (*text).into(),
                flag: None,
            });
        }
        transcript
    }

    fn ask(kind: AskKind, text: &str) -> String {
        let context = ClassContext {
            subject: "Дискретна математика".into(),
            ..ClassContext::default()
        };
        let messages = helper_messages(kind, &context, &transcript(), &Notes::default(), text);
        assert_eq!(messages.len(), 2);
        assert!(messages[0].content.contains("never write answers"));
        messages[1].content.clone()
    }

    #[test]
    fn simplify_falls_back_to_the_last_phrase() {
        assert!(ask(AskKind::Simplify, "").contains("simply, in 2 or 3 sentences: Хто знає"));
        assert!(ask(AskKind::Simplify, "ребро").contains("sentences: ребро"));
    }

    #[test]
    fn every_kind_names_the_class_and_carries_text() {
        assert!(ask(AskKind::Recap, "").contains("Граф — це вершини"));
        assert!(ask(AskKind::Topic, "").contains("Вступ до графів."));
        assert!(ask(AskKind::Free, "що таке ребро?").contains("що таке ребро?"));
        let summary = ask(AskKind::Summary, "");
        assert!(summary.contains("Markdown") && summary.contains("do not answer"));
        for kind in [
            AskKind::Recap,
            AskKind::Topic,
            AskKind::Free,
            AskKind::Summary,
        ] {
            assert!(ask(kind, "x").contains("Дискретна математика"));
        }
    }
}
