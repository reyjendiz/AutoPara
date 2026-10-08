package app.autopara.core.importer

import java.text.Normalizer
import java.time.DayOfWeek
import java.time.LocalDate
import java.time.temporal.TemporalAdjusters

/**
 * Text and time helpers for the timetable document. A direct port of `importer/normalize.py`: the
 * source file is Ukrainian and inconsistent (backtick apostrophes, mixed Cyrillic/Latin numerals),
 * but day names are accepted in several languages.
 */
object Normalize {
    /** Pair number -> canonical start time. Pairs 7-10 continue the rhythm into the evening. */
    private val TIME_TO_PAIR = linkedMapOf(
        "8.00" to 1, "9.30" to 2, "11.20" to 3, "13.00" to 4, "14.40" to 5,
        "16.10" to 6, "17.40" to 7, "19.10" to 8, "20.40" to 9, "22.10" to 10,
    )
    private val PAIR_TO_TIME: Map<Int, String> = TIME_TO_PAIR.entries.associate { (k, v) -> v to k }

    const val DEFAULT_CLASS_MINUTES = 80

    private const val APOSTROPHES = "`’ʼʹ‘´"
    private val URL_RE = Regex("""https?://[^\s<>"']+""")
    private const val TRAILING_PUNCT = ".,;:)]}"
    private val CLOCK_RE = Regex("""(\d{1,2})\s*[.:]\s*(\d{2})""")
    private val SHORT_TIME_RE = Regex("""^(\d{1,2})[.:](\d{2})$""")
    private val WHITESPACE = Regex("""\s+""")

    private fun two(n: Int): String = n.toString().padStart(2, '0')

    /** `HH:MM`, always with ASCII digits whatever the device locale is. */
    fun hhmm(hour: Int, minute: Int): String = "${two(hour)}:${two(minute)}"

    /** NFC-normalize, fold apostrophe variants, collapse whitespace. */
    fun normalizeText(value: String?): String {
        var text = Normalizer.normalize(value ?: "", Normalizer.Form.NFC)
        for (ch in APOSTROPHES) text = text.replace(ch, '\'')
        return WHITESPACE.replace(text, " ").trim()
    }

    // ------------------------------------------------------------------ day names

    private val dayAliases = HashMap<String, Int>()

    private fun register(index: Int, vararg names: String) {
        for (name in names) dayAliases[normalizeText(name).lowercase()] = index
    }

    init {
        register(0, "понеділок", "понеділка", "пн", "понедельник", "monday", "mon", "poniedzialek",
            "poniedziałek", "montag", "lunes", "lundi", "lunedi", "lunedì", "luni")
        register(1, "вівторок", "вт", "вторник", "tuesday", "tue", "tues", "wtorek", "dienstag",
            "martes", "mardi", "martedi", "martedì", "marti")
        register(2, "середа", "середи", "ср", "среда", "wednesday", "wed", "sroda", "środa",
            "mittwoch", "miercoles", "miércoles", "mercredi", "mercoledi", "mercoledì")
        register(3, "четвер", "чт", "четверг", "thursday", "thu", "thur", "thurs", "czwartek",
            "donnerstag", "jueves", "jeudi", "giovedi", "giovedì", "joi")
        register(4, "п'ятниця", "п'ятниці", "пт", "пятница", "friday", "fri", "piatek", "piątek",
            "freitag", "viernes", "vendredi", "venerdi", "venerdì", "vineri")
        register(5, "субота", "сб", "суббота", "saturday", "sat", "sobota", "samstag", "sonnabend",
            "sabado", "sábado", "samedi", "sabato", "sambata", "sâmbătă")
        register(6, "неділя", "нд", "вс", "воскресенье", "sunday", "sun", "niedziela", "sonntag",
            "domingo", "dimanche", "domenica", "duminica", "duminică")
    }

    /** Longest first so "субота" wins over the shorter "сб" when a cell holds extra text. */
    private val aliasesByLength: List<String> by lazy { dayAliases.keys.sortedByDescending { it.length } }

    private fun dayKey(value: String): String =
        normalizeText(value).lowercase().trim(' ', '.', ',', ':', ';', '-', '–', '—', '(', ')', '[', ']')

    /** Map a day name in any supported language to a weekday (0 = Monday), or null. */
    fun dayIndex(value: String): Int? {
        val needle = dayKey(value)
        if (needle.isEmpty()) return null
        dayAliases[needle]?.let { return it }
        // A cell may carry more than the bare name ("Понеділок 02.09"); match on its opening word.
        for (alias in aliasesByLength) {
            if (alias.length >= 3 && needle.startsWith(alias)) return dayAliases.getValue(alias)
        }
        return null
    }

    fun dayOfWeek(index: Int): DayOfWeek = DayOfWeek.of(index % 7 + 1)

    fun weekStart(day: LocalDate): LocalDate = day.with(TemporalAdjusters.previousOrSame(DayOfWeek.MONDAY))

    // ------------------------------------------------------------------ times

    /** `H.MM` / `H:MM` -> canonical `HH:MM`, or null. */
    fun parseTime(value: String): String? {
        val match = SHORT_TIME_RE.matchEntire(normalizeText(value)) ?: return null
        val hour = match.groupValues[1].toInt()
        val minute = match.groupValues[2].toInt()
        if (hour !in 0..23 || minute !in 0..59) return null
        return hhmm(hour, minute)
    }

    /** Pull a start and an optional end time out of a time cell. */
    fun parseTimeRange(value: String): Pair<String?, String?> {
        val found = CLOCK_RE.findAll(normalizeText(value)).mapNotNull { m ->
            val hour = m.groupValues[1].toInt()
            val minute = m.groupValues[2].toInt()
            if (hour in 0..23 && minute in 0..59) hhmm(hour, minute) else null
        }.toList()
        val start = found.firstOrNull()
        val end = if (found.size > 1 && found[1] > found[0]) found[1] else null
        return start to end
    }

    /** The pair number a start time is, if it is exactly on the university's grid. */
    fun pairFromTime(value: String): Int? {
        val text = normalizeText(value).replace(':', '.')
        TIME_TO_PAIR[text]?.let { return it }
        val canonical = parseTime(value) ?: return null
        val (hour, minute) = canonical.split(":")
        return TIME_TO_PAIR["${hour.toInt()}.$minute"]
    }

    fun pairStartTime(pair: Int): String {
        val raw = PAIR_TO_TIME[pair] ?: return ""
        val (hour, minute) = raw.split(".")
        return "${two(hour.toInt())}:$minute"
    }

    /** Which grid row an arbitrary time belongs in: the last slot that has already begun. */
    fun pairSlot(hhmm: String): Int {
        pairFromTime(hhmm)?.let { return it }
        var slot = 1
        for (pair in PAIR_TO_TIME.keys.sorted()) if (pairStartTime(pair) <= hhmm) slot = pair
        return slot
    }

    /** The highest pair slot that begins inside `[start, end)`. */
    fun lastPairCovered(startTime: String, endTime: String): Int {
        var last = pairSlot(startTime)
        for (pair in PAIR_TO_TIME.keys) {
            val slot = pairStartTime(pair)
            if (startTime <= slot && slot < endTime) last = maxOf(last, pair)
        }
        return last
    }

    fun addMinutes(hhmm: String, minutes: Int): String {
        val (hour, minute) = hhmm.split(":").map { it.toInt() }
        val total = minOf(hour * 60 + minute + minutes, 24 * 60 - 1)
        return hhmm(total / 60, total % 60)
    }

    // ------------------------------------------------------------------ urls

    fun cleanUrl(value: String?): String = (value ?: "").trim().trimEnd(*TRAILING_PUNCT.toCharArray())

    fun findUrls(text: String?): List<String> =
        URL_RE.findAll(text ?: "").map { cleanUrl(it.value) }.toList()

    /** Order-preserving dedupe: every link appears twice in a Word hyperlink. */
    fun dedupe(values: List<String>): List<String> = values.filter { it.isNotEmpty() }.distinct()

    fun isTeacherLine(line: String): Boolean {
        val text = normalizeText(line)
        return text.startsWith("(") && text.endsWith(")")
    }

    // ------------------------------------------------------------------ courses

    private val ROMAN = listOf("I", "II", "III", "IV", "V", "VI", "VII", "VIII")

    /** Course headings are matched by ordinal, never by text; the label is always the same. */
    fun courseLabel(ordinal: Int): String {
        val numeral = ROMAN.getOrNull(ordinal - 1) ?: ordinal.toString()
        return "$numeral курс"
    }
}
