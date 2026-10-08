package app.autopara.core.schedule

import app.autopara.core.importer.Normalize
import app.autopara.core.model.Lesson
import java.time.LocalDate

/**
 * Which classes the calendar shows: a text search and a subject filter. One rule in one place, so
 * the day list, the week, the month and "jump to next match" cannot disagree. Port of
 * `core/filters.py`.
 *
 * Forgiving the way people are: case and apostrophe variants do not matter, and every typed word
 * has to appear in the subject or the teacher, in any order.
 */
data class LessonFilter(val text: String = "", val subject: String = "") {
    val isActive: Boolean get() = text.isNotBlank() || subject.isNotEmpty()

    fun matches(lesson: Lesson): Boolean {
        if (subject.isNotEmpty() && fold(lesson.subject) != fold(subject)) return false
        val words = fold(text).split(' ').filter { it.isNotEmpty() }
        if (words.isEmpty()) return true
        val haystack = "${fold(lesson.subject)} ${fold(lesson.teacher)}"
        return words.all { it in haystack }
    }

    companion object {
        /** How far "next match" looks before giving up: a year and a bit of a timetable. */
        const val SEARCH_HORIZON_DAYS = 400

        private fun fold(text: String): String = Normalize.normalizeText(text).lowercase()

        private const val ALPHABET = "абвгґдеєжзиіїйклмнопрстуфхцчшщьюяabcdefghijklmnopqrstuvwxyz"

        /** Compares in Ukrainian alphabetical order (code points would put "і" before "а"). */
        private val ukrainianOrder = Comparator<String> { a, b ->
            val left = fold(a)
            val right = fold(b)
            val common = minOf(left.length, right.length)
            for (i in 0 until common) {
                val diff = rank(left[i]) - rank(right[i])
                if (diff != 0) return@Comparator diff
            }
            left.length - right.length
        }

        private fun rank(char: Char): Int {
            val index = ALPHABET.indexOf(char)
            return if (index >= 0) index else ALPHABET.length + char.code
        }

        /** Every distinct subject once, alphabetically -- what the subject drop-down offers. */
        fun subjects(lessons: List<Lesson>): List<String> {
            val seen = LinkedHashMap<String, String>()
            for (lesson in lessons) seen.putIfAbsent(fold(lesson.subject), Normalize.normalizeText(lesson.subject))
            return seen.values.sortedWith(ukrainianOrder)
        }
    }
}

fun List<Lesson>.filteredBy(filter: LessonFilter): List<Lesson> =
    if (filter.isActive) filter { filter.matches(it) } else toList()

/** The nearest date after (direction 1) or before (-1) [start] with a matching class, or null. */
fun findMatch(lessons: List<Lesson>, filter: LessonFilter, start: LocalDate, direction: Int = 1): LocalDate? {
    val candidates = lessons.filteredBy(filter)
    if (candidates.isEmpty()) return null
    val step = if (direction >= 0) 1L else -1L
    var day = start.plusDays(step)
    repeat(LessonFilter.SEARCH_HORIZON_DAYS) {
        if (candidates.any { it.occursOn(day) }) return day
        day = day.plusDays(step)
    }
    return null
}
