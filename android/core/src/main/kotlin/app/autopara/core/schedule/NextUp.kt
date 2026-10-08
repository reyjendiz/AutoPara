package app.autopara.core.schedule

import app.autopara.core.model.Lesson
import java.time.Duration
import java.time.LocalDateTime
import kotlin.math.roundToInt

/**
 * The class that is on now, or the next one to start today. Data only -- the UI formats it with
 * string resources so the wording follows the device language. Port of `core/nextup.py`.
 */
data class NextUp(
    val lesson: Lesson,
    /** It has started and not ended. */
    val ongoing: Boolean,
    /** Minutes until it starts or, when [ongoing], until it ends. */
    val minutes: Int,
) {
    /** Beyond [COUNTDOWN_LIMIT_MINUTES] "in 3 hours" stops being useful: the clock time says more. */
    val showsCountdown: Boolean get() = ongoing || minutes <= COUNTDOWN_LIMIT_MINUTES

    companion object {
        const val COUNTDOWN_LIMIT_MINUTES = 90

        /**
         * @param settled ids of today's classes that were skipped: they are not "next".
         */
        fun find(lessons: List<Lesson>, now: LocalDateTime, settled: Set<Long> = emptySet()): NextUp? {
            val today = now.toLocalDate()
            val candidates = lessons.filter {
                it.occursOn(today) && it.id !in settled && it.endsOn(today).isAfter(now)
            }
            val lesson = candidates.minByOrNull { it.start } ?: return null
            val start = lesson.startsOn(today)
            val end = lesson.endsOn(today)
            return if (!start.isAfter(now)) {
                NextUp(lesson, true, minutesBetween(now, end))
            } else {
                NextUp(lesson, false, minutesBetween(now, start))
            }
        }

        private fun minutesBetween(from: LocalDateTime, to: LocalDateTime): Int =
            maxOf(1, (Duration.between(from, to).seconds / 60.0).roundToInt())
    }
}
