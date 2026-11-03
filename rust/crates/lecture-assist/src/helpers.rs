//! Prompts for the on-demand helpers and the end-of-class summary (plan step 7.7 e, f).

use crate::llm::Message;
use crate::notes::{Notes, Transcript};
use crate::{AskKind, ClassContext};

/// Answering the teacher's questions is the owner's choice (`AskKind::Answer`, off by default).
const SYSTEM: &str = "You help a student during a lecture. Answer in Ukrainian whatever the \
    lecture's language. Be brief and use plain text.";

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
        // Nothing typed ("Відповісти зараз"): whatever was said last holds the question.
        AskKind::Answer if text.trim().is_empty() => format!(
            "{class}\nThe teacher has just said this; answer the question in it, or the last thing \
             asked: {}\nWrite the answer the student can say aloud, as short as possible: a few \
             words, or one short sentence when words are not enough. No preamble, no \
             explanation. Use what was said in class first, general knowledge where it says \
             nothing.\n\nSummary so far: {summary}\nRecent:\n{}",
            transcript.since(transcript.end_ms().saturating_sub(15_000)),
            transcript.window(3)
        ),
        AskKind::Answer => format!(
            "{class}\nThe teacher just asked the class: {text}\nWrite the answer the student can \
             say aloud, as short as possible: a few words, or one short sentence when words are \
             not enough. No preamble, no explanation. Use what was said in class first, general \
             knowledge where it says nothing.\n\nSummary so far: {summary}\nRecent:\n{}",
            transcript.window(3)
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
        assert!(messages[0].content.contains("Ukrainian"));
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
        let answer = ask(AskKind::Answer, "Хто знає, що таке дерево?");
        assert!(answer.contains("asked the class: Хто знає, що таке дерево?"));
        assert!(answer.contains("say aloud, as short as possible"));
        let now = ask(AskKind::Answer, "");
        assert!(
            now.contains("answer the question in it") && now.contains("Граф"),
            "{now}"
        );
        let summary = ask(AskKind::Summary, "");
        assert!(summary.contains("Markdown") && summary.contains("do not answer"));
        for kind in [
            AskKind::Recap,
            AskKind::Topic,
            AskKind::Free,
            AskKind::Answer,
            AskKind::Summary,
        ] {
            assert!(ask(kind, "x").contains("Дискретна математика"));
        }
    }
}
