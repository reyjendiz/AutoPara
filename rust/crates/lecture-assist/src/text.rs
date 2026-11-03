//! Cleaning recogniser output and building prompts (plan steps 7.5 and 7.7 d).

use crate::{ClassContext, FlagKind};

/// Phrases whisper makes up on silence or music, mostly from subtitle credits in its training
/// data. Matched after [`normalise`].
const PHANTOMS: &[&str] = &[
    "дякую за перегляд",
    "дякуємо за перегляд",
    "підписуйтесь на канал",
    "спасибо за просмотр",
    "продолжение следует",
    "субтитры сделал",
    "субтитры создавал",
    "редактор субтитров",
    "корректор а",
    "подписывайтесь на канал",
    "thank you for watching",
    "thanks for watching",
    "please subscribe",
    "subtitles by the amara",
    "music",
    "музыка",
    "музика",
];

/// Lower case, letters, digits and single spaces only.
pub fn normalise(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// A short line that is only a known phantom phrase.
pub fn is_phantom(text: &str) -> bool {
    let norm = normalise(text);
    norm.is_empty()
        || PHANTOMS
            .iter()
            .any(|p| norm.starts_with(p) && norm.len() <= p.len() + 24)
}

/// Collapse a phrase repeated three or more times in a row ("так так так так") to one copy,
/// whisper's usual loop when it loses track.
pub fn collapse_repetitions(text: &str) -> String {
    let mut words: Vec<&str> = text.split_whitespace().collect();
    for n in 1..=6 {
        let mut i = 0;
        while i + 3 * n <= words.len() {
            let same = |a: usize, b: usize| {
                (0..n).all(|k| normalise(words[a + k]) == normalise(words[b + k]))
            };
            let mut repeats = 1;
            while i + (repeats + 1) * n <= words.len() && same(i, i + repeats * n) {
                repeats += 1;
            }
            if repeats >= 3 {
                words.drain(i + n..i + repeats * n);
            }
            i += 1;
        }
    }
    words.join(" ")
}

/// Drop the words at the start of `next` that repeat the end of `previous` (the overlap after
/// a forced cut, or a partial already shown).
pub fn strip_overlap(previous: &str, next: &str) -> String {
    let prev: Vec<String> = previous.split_whitespace().map(normalise).collect();
    let words: Vec<&str> = next.split_whitespace().collect();
    let norm: Vec<String> = words.iter().map(|w| normalise(w)).collect();
    let longest = (1..=prev.len().min(norm.len()).min(12))
        .rev()
        .find(|&k| prev[prev.len() - k..] == norm[..k])
        .unwrap_or(0);
    words[longest..].join(" ")
}

/// Everything above in order; `None` when nothing worth showing is left.
pub fn clean(previous: &str, text: &str) -> Option<String> {
    let text = collapse_repetitions(text.trim());
    if is_phantom(&text) {
        return None;
    }
    let text = strip_overlap(previous, &text);
    (!normalise(&text).is_empty()).then_some(text)
}

/// whisper's initial prompt: the class and the user's glossary, so names and terms come out
/// spelled right. Kept well under its 224-token limit.
pub fn initial_prompt(context: &ClassContext, glossary: &[String]) -> String {
    let mut parts = Vec::new();
    if !context.subject.is_empty() {
        parts.push(context.subject.clone());
    }
    if !context.teacher.is_empty() {
        parts.push(context.teacher.clone());
    }
    let mut prompt = parts.join(". ");
    for term in glossary.iter().map(|t| t.trim()).filter(|t| !t.is_empty()) {
        if prompt.chars().count() + term.chars().count() > 400 {
            break;
        }
        prompt.push_str(if prompt.is_empty() { "" } else { ", " });
        prompt.push_str(term);
    }
    if !prompt.is_empty() {
        prompt.push('.');
    }
    prompt
}

/// Words that mark a task or homework, in Ukrainian, Russian and English stems.
const TASK_WORDS: &[&str] = &[
    "домашн",
    "завдан",
    "здат",
    "здайте",
    "дедлайн",
    "до наступно",
    "підготуйте",
    "задани",
    "домашк",
    "сдат",
    "сдайте",
    "подготовьте",
    "к следующ",
    "homework",
    "assignment",
    "deadline",
    "due ",
    "submit",
    "prepare for next",
];

/// What a teacher asks the group with, for when the recogniser leaves out the "?" (it often does
/// in Ukrainian).
const QUESTION_CUES: &[&str] = &[
    "хто знає",
    "хто може",
    "хто скаже",
    "хто пам'ятає",
    "хто пам’ятає",
    "хто відповість",
    "як ви думаєте",
    "як ви вважаєте",
    "що ви думаєте",
    "чи може хтось",
    "скажіть",
    "назвіть",
    "поясніть",
    "наведіть приклад",
    "кто знает",
    "кто может",
    "кто скажет",
    "кто помнит",
    "кто ответит",
    "как вы думаете",
    "как вы считаете",
    "что вы думаете",
    "может кто-то",
    "скажите",
    "назовите",
    "объясните",
    "приведите пример",
];

/// A question or a task said in class. Only marks it; answering is the engine's opt-in.
pub fn detect_flag(text: &str) -> Option<FlagKind> {
    let lower = format!("{} ", text.to_lowercase());
    if TASK_WORDS.iter().any(|w| lower.contains(w)) {
        Some(FlagKind::Task)
    } else if text.contains('?') || QUESTION_CUES.iter().any(|c| lower.contains(c)) {
        Some(FlagKind::Question)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phantoms_are_dropped() {
        assert!(is_phantom("Дякую за перегляд!"));
        assert!(is_phantom("Субтитры сделал DimaTorzok"));
        assert!(is_phantom(" [Music] "));
        assert!(is_phantom("..."));
        assert!(!is_phantom(
            "Дякую за увагу, переходимо до другої частини лекції про графи"
        ));
    }

    #[test]
    fn repetitions_collapse() {
        assert_eq!(collapse_repetitions("так так так так так"), "так");
        assert_eq!(
            collapse_repetitions("і далі, і далі, і далі, і далі ми бачимо"),
            "і далі, ми бачимо"
        );
        assert_eq!(collapse_repetitions("very very good"), "very very good");
    }

    #[test]
    fn overlap_is_stripped() {
        assert_eq!(
            strip_overlap(
                "ми розглянемо граф і його вершини",
                "Його вершини, та ребра"
            ),
            "та ребра"
        );
        assert_eq!(strip_overlap("", "нове речення"), "нове речення");
        assert_eq!(clean("старий текст", "старий текст"), None);
    }

    #[test]
    fn prompt_names_the_class() {
        let context = ClassContext {
            subject: "Дискретна математика".into(),
            teacher: "Іваненко О. П.".into(),
            ..ClassContext::default()
        };
        let prompt = initial_prompt(&context, &["граф".into(), " ".into(), "ізоморфізм".into()]);
        assert_eq!(
            prompt,
            "Дискретна математика. Іваненко О. П., граф, ізоморфізм."
        );
        assert_eq!(initial_prompt(&ClassContext::default(), &[]), "");
        let long: Vec<String> = (0..200).map(|i| format!("термін{i}")).collect();
        assert!(initial_prompt(&context, &long).chars().count() <= 420);
    }

    #[test]
    fn flags_questions_and_tasks() {
        assert_eq!(
            detect_flag("Хто може сказати, що таке дерево?"),
            Some(FlagKind::Question)
        );
        assert_eq!(
            detect_flag("Домашнє завдання: вправи 3 і 4 до п'ятниці."),
            Some(FlagKind::Task)
        );
        assert_eq!(
            detect_flag("The homework is due Monday."),
            Some(FlagKind::Task)
        );
        assert_eq!(detect_flag("Отже, дерево це зв'язний граф."), None);
        // Mid-text, and without the mark whisper often leaves out.
        assert_eq!(
            detect_flag("Що таке граф? Отже, почнемо з вершин."),
            Some(FlagKind::Question)
        );
        assert_eq!(
            detect_flag("Хто знає, як називається цей метод"),
            Some(FlagKind::Question)
        );
        assert_eq!(
            detect_flag("Как вы думаете, почему так происходит"),
            Some(FlagKind::Question)
        );
    }
}
