//! Prompts for the on-demand helpers and the end-of-class summary (plan step 7.7 e, f).

use crate::llm::Message;
use crate::notes::{Notes, Transcript};
use crate::{AskKind, ClassContext};

/// Answering the teacher's questions is the owner's choice (`AskKind::Answer`, on by default).
const SYSTEM: &str = "You help a student during a lecture. Answer in Ukrainian whatever the \
    lecture's language, unless told otherwise. Be brief and use plain text.";

/// The language an answer is said in: the question's, Russian or Ukrainian. The models lean to
/// Ukrainian when only asked to "match the question", so the code decides, from letters and short
/// words only one of the two languages uses.
fn language_of(text: &str) -> &'static str {
    const RUSSIAN: &[&str] = &[
        "что",
        "почему",
        "кто",
        "как",
        "это",
        "сколько",
        "зачем",
        "когда",
        "какой",
        "какая",
        "такое",
        "вы",
        "нужен",
        "ли",
    ];
    const UKRAINIAN: &[&str] = &[
        "що",
        "чому",
        "хто",
        "як",
        "це",
        "скільки",
        "навіщо",
        "коли",
        "який",
        "яка",
        "таке",
        "ви",
        "чи",
        "потрібен",
    ];
    let lower = text.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphabetic())
        .filter(|w| !w.is_empty())
        .collect();
    let score = |letters: &str, list: &[&str]| {
        lower.chars().filter(|c| letters.contains(*c)).count()
            + words.iter().filter(|w| list.contains(w)).count()
    };
    if score("ыэъё", RUSSIAN) > score("іїєґ", UKRAINIAN) {
        "Russian"
    } else {
        "Ukrainian"
    }
}

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
    let mut system = SYSTEM.to_string();
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
        AskKind::Answer => {
            // Nothing typed ("Відповісти зараз"): whatever was said last holds the question.
            let typed = !text.trim().is_empty();
            let question = if typed {
                text.to_string()
            } else {
                transcript.since(transcript.end_ms().saturating_sub(15_000))
            };
            let asked = if typed {
                "The teacher just asked the class"
            } else {
                "The teacher has just said this; answer the question in it, or the last thing asked"
            };
            let language = language_of(&question);
            let user = format!(
                "{class}\n{asked}: {question}\nWrite the answer the student can say aloud, in {}, \
                 as short as possible: a few words, or one short sentence when words are not \
                 enough. No preamble, no explanation. Use what was said in class first, general \
                 knowledge where it says nothing.\n\nSummary so far: {summary}\nRecent:\n{}",
                language,
                transcript.window(3)
            );
            // A Ukrainian-tuned model drifted back to Ukrainian under the general "in Ukrainian";
            // the answer's own system line names the one language to use.
            system = format!(
                "You help a student during a lecture. Reply only in {language}. Be brief and use \
                 plain text."
            );
            user
        }
        AskKind::Summary => format!(
            "{class}\nWrite the end-of-class summary in Markdown with sections for the topic, \
             key points, definitions, deadlines and homework, and the questions or tasks heard \
             in class (list them, do not answer them).\n\nSummary so far: {summary}\n\
             Notes: {}\nThe last part of the lecture:\n{}",
            serde_json::to_string(notes).unwrap_or_default(),
            transcript.window(10)
        ),
    };
    vec![Message::system(system), Message::user(user)]
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
        assert!(answer.contains("say aloud, in Ukrainian"));
        assert_eq!(language_of("Почему игра так важна для ребенка"), "Russian");
        assert_eq!(
            language_of("Сколько лет длится дошкольный возраст"),
            "Russian"
        );
        assert_eq!(language_of("Чому гра така важлива для дитини"), "Ukrainian");
        assert_eq!(language_of("Назвіть основні методи виховання"), "Ukrainian");
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
