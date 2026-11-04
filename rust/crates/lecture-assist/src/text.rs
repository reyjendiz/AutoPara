//! Cleaning recogniser output and building prompts (plan steps 7.5 and 7.7 d).

use crate::FlagKind;

/// Phrases whisper makes up on silence or music, mostly from subtitle credits in its training
/// data. Matched after [`normalise`].
const PHANTOMS: &[&str] = &[
    "дякую за перегляд",
    "дякуємо за перегляд",
    "дякуємо за підтримку",
    "дякую за підтримку",
    "підпишись на канал",
    "підписуйтеся на канал",
    "ставте лайк",
    "не забудьте підписатися",
    "редактор субтитрів",
    "субтитри зроблено",
    "субтитрувальниця",
    "субтитрувальник",
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

/// Whole lines whisper makes up on pauses and noise: a lone "Дякую." came 1-5 times in 7 min of
/// clean speech and up to 66 times in noise, never in the reference text (docs/ASSIST_BENCH.md).
/// Only the bare word: "Дякую, сідайте" stays.
const LONE_PHANTOMS: &[&str] = &["дякую", "дякую вам", "спасибо"];

/// A short line that is only a known phantom phrase.
pub fn is_phantom(text: &str) -> bool {
    let norm = normalise(text);
    norm.is_empty()
        || LONE_PHANTOMS.contains(&norm.as_str())
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

/// whisper's initial prompt: only the user's own glossary. whisper.cpp repeats its prompt on noise,
/// so the class's subject and teacher, put there unasked, came back as phantom lines (as
/// Magerko/copilot found too). Kept well under its 224-token limit.
pub fn initial_prompt(glossary: &[String]) -> String {
    let mut prompt = String::new();
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
    "до наступного заняття",
    "до наступної пари",
    "до наступного разу",
    "на наступне заняття",
    "підготуйте",
    "задани",
    "домашк",
    "сдат",
    "сдайте",
    "подготовьте",
    "к следующему занятию",
    "к следующей паре",
    "к следующему разу",
    "на следующее занятие",
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

/// Words a question opens with, Ukrainian then Russian; with three words or more, a sentence that
/// starts with one is asked even without a "?" (lists adapted from Magerko/copilot, MIT).
const LEAD_WORDS: &[&str] = &[
    "що",
    "чим",
    "чому",
    "навіщо",
    "як",
    "коли",
    "де",
    "куди",
    "звідки",
    "який",
    "яка",
    "яке",
    "які",
    "яким",
    "яких",
    "хто",
    "кого",
    "скільки",
    "чи",
    "розкажіть",
    "опишіть",
    "порівняйте",
    "что",
    "чем",
    "почему",
    "зачем",
    "как",
    "когда",
    "где",
    "куда",
    "откуда",
    "какой",
    "какая",
    "какое",
    "какие",
    "каким",
    "каких",
    "кто",
    "кого",
    "сколько",
    "расскажите",
    "опишите",
    "сравните",
];

/// Openings that look like a question but tell instead ("Як ми бачили, ...").
const TELLING: &[&str] = &[
    "як ми",
    "як бачите",
    "як відомо",
    "як я",
    "що стосується",
    "коли ми",
    "де ми",
    "как мы",
    "как видите",
    "как известно",
    "как я",
    "что касается",
    "когда мы",
    "где мы",
];

/// "You" in Ukrainian and Russian: a sentence that opens with a name and speaks to someone.
const SECOND_PERSON: &[&str] = &[
    "ти",
    "тобі",
    "тебе",
    "тобою",
    "твій",
    "твоя",
    "твоє",
    "твої",
    "ви",
    "вам",
    "вас",
    "вами",
    "ваш",
    "ваша",
    "ваше",
    "ваші",
    "ты",
    "тебя",
    "тобой",
    "твой",
    "твоё",
    "твои",
    "вы",
    "ваши",
];

/// Checking the line, not asking the class.
const SMALL_TALK: &[&str] = &[
    "як справи",
    "мене чути",
    "чуєте мене",
    "чути мене",
    "мене видно",
    "как дела",
    "меня слышно",
    "слышите меня",
    "слышно меня",
    "меня видно",
];

/// One sentence asks: a question word first, or "ли" second, and long enough to mean it. A first
/// word set off by a comma (a name, "Отже,") is skipped, "Артеме, скільки тобі років", and so is a
/// joining "а" or "но".
fn asks(sentence: &str) -> bool {
    let mut raw = sentence.split_whitespace().peekable();
    let addressed = raw.peek().is_some_and(|w| w.ends_with(','));
    if addressed {
        raw.next();
    }
    let mut words: Vec<String> = raw.map(normalise).filter(|w| !w.is_empty()).collect();
    // "А хто з вас...", "Но почему...": a joining word before the question word.
    if words.len() > 3 && ["а", "і", "й", "але", "и", "но", "ну"].contains(&words[0].as_str())
    {
        words.remove(0);
    }
    // "Дитина, яка грає, ..." is no question; after a name, only "you" or a word that never joins
    // a clause makes one: "Артеме, скільки тобі років", "Артеме, скільки років триває ...".
    let unambiguous = [
        "скільки",
        "навіщо",
        "чому",
        "хто",
        "сколько",
        "зачем",
        "почему",
        "кто",
    ];
    if addressed
        && !words.iter().any(|w| SECOND_PERSON.contains(&w.as_str()))
        && !words
            .first()
            .is_some_and(|w| unambiguous.contains(&w.as_str()))
    {
        return false;
    }
    let opening = words.iter().take(2).cloned().collect::<Vec<_>>().join(" ");
    let first = words.first().map_or("", String::as_str);
    words.len() >= 3
        && !TELLING.iter().any(|t| opening.starts_with(t))
        && (LEAD_WORDS.contains(&first) || words.get(1).is_some_and(|w| w == "ли"))
}

/// A question or a task said in class. Only marks it; answering is the engine's opt-in.
pub fn detect_flag(text: &str) -> Option<FlagKind> {
    let lower = format!("{} ", text.to_lowercase());
    let small_talk = text.split_whitespace().count() <= 8
        && SMALL_TALK.iter().any(|s| normalise(text).contains(s));
    if TASK_WORDS.iter().any(|w| lower.contains(w)) {
        Some(FlagKind::Task)
    } else if small_talk {
        None
    } else if text.contains('?')
        || QUESTION_CUES.iter().any(|c| lower.contains(c))
        || text.split(['.', '!', '?', ';']).any(asks)
    {
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
        assert!(is_phantom("Дякую."));
        assert!(is_phantom("Субтитрувальниця Оля Шор"));
        assert!(!is_phantom("Дякую, сідайте."));
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
    fn prompt_is_the_glossary_only() {
        let prompt = initial_prompt(&["граф".into(), " ".into(), "ізоморфізм".into()]);
        assert_eq!(prompt, "граф, ізоморфізм.");
        assert_eq!(initial_prompt(&[]), "");
        let long: Vec<String> = (0..200).map(|i| format!("термін{i}")).collect();
        assert!(initial_prompt(&long).chars().count() <= 420);
    }

    /// What a teacher says in a pedagogy class, as whisper writes it: often without the "?".
    #[test]
    fn ukrainian_and_russian_questions_are_told_from_statements() {
        let questions = [
            "Хто знає, що таке зона найближчого розвитку",
            "Артеме, скільки років триває дошкільний вік",
            "Які функції виконує гра в розвитку дитини",
            "Чому важлива співпраця з батьками",
            "Назвіть основні методи виховання",
            "Як ви думаєте, навіщо дітям рухливі ігри",
            "Чи можете ви навести приклад дидактичної гри",
            "Коли був заснований перший дитячий садок в Україні",
            "Що таке дидактична гра",
            "Олено, як ти розумієш поняття гри",
            "Кто знает, что такое зона ближайшего развития",
            "Сколько лет длится дошкольный возраст",
            "Почему игра так важна для ребёнка",
            "Назовите основные принципы воспитания",
            "Знаете ли вы, кто такой Макаренко",
            "Как вы думаете, зачем детям нужен режим дня",
            "Что такое дидактическая игра",
            "Ирина, как вы понимаете понятие игры",
            "Сьогодні ми поговоримо. А хто з вас читав Сухомлинського",
        ];
        for line in questions {
            assert_eq!(detect_flag(line), Some(FlagKind::Question), "{line}");
        }
        let statements = [
            "Сьогодні ми поговоримо про розвиток мовлення дошкільників.",
            "Як ми бачили минулого разу, гра є провідною діяльністю.",
            "Ми знаємо, що діти вчаться через досвід.",
            "Отже, переходимо до наступної теми.",
            "Дитина, яка грає, розвивається швидше.",
            "Дитина, що грає, розвивається швидше.",
            "Вихователь, коли планує заняття, враховує вік дітей.",
            "Мене чути?",
            "Сегодня мы говорим о развитии речи.",
            "Как мы видели, игра является ведущей деятельностью.",
            "Мы знаем, что дети учатся на опыте.",
            "Ребёнок, который играет, развивается быстрее.",
            "Воспитатель, когда планирует занятие, учитывает возраст детей.",
            "Меня слышно?",
        ];
        for line in statements {
            assert_eq!(detect_flag(line), None, "{line}");
        }
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
        // A question word first, or "ли" second, with no mark at all.
        assert_eq!(
            detect_flag("Артеме, скільки тобі років"),
            Some(FlagKind::Question)
        );
        assert_eq!(detect_flag("Отже, як ми бачили, граф має вершини."), None);
        assert_eq!(detect_flag("Ми знаємо, що граф має вершини."), None);
        assert_eq!(
            detect_flag("Скільки років триває навчання в університеті"),
            Some(FlagKind::Question)
        );
        assert_eq!(
            detect_flag("Знаете ли вы теорему Пифагора"),
            Some(FlagKind::Question)
        );
        assert_eq!(detect_flag("Як ми бачили, граф має вершини."), None);
        assert_eq!(detect_flag("Мене чути?"), None);
    }
}
