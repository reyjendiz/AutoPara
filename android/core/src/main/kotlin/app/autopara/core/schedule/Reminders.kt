package app.autopara.core.schedule

import app.autopara.core.model.Lesson
import app.autopara.core.model.Occurrence
import java.time.LocalDate
import java.time.LocalDateTime

/**
 * When to remind about a class. Pure functions of the lessons and the clock, so the exact-minute
 * rule is tested without a device.
 */
object Reminders {
    /** The user-visible promise: a notification exactly this many minutes before a class starts. */
    const val LEAD_MINUTES = 1L

    /** How many days ahead alarms are armed. They are re-armed whenever one fires and on every boot. */
    const val HORIZON_DAYS = 7L

    /** Android keeps a per-app alarm budget; this keeps a very dense timetable under it. */
    const val MAX_ALARMS = 60

    /** A reminder armed for [occurrence], due at [triggerAt]. */
    data class Reminder(val occurrence: Occurrence, val triggerAt: LocalDateTime)

    /**
     * Every reminder due after [now] and within [HORIZON_DAYS], soonest first, capped at
     * [MAX_ALARMS]. A class that already started, one with no usable link and any date in
     * [skipped] are left out.
     *
     * @param skipped (lessonId, date) pairs the user dismissed.
     */
    fun upcoming(
        lessons: List<Lesson>,
        now: LocalDateTime,
        skipped: Set<Pair<Long, LocalDate>> = emptySet(),
        leadMinutes: Long = LEAD_MINUTES,
        horizonDays: Long = HORIZON_DAYS,
        activeFrom: LocalDateTime? = null,
    ): List<Reminder> {
        val result = ArrayList<Reminder>()
        val firstDay = now.toLocalDate()
        for (offset in 0..horizonDays) {
            val day = firstDay.plusDays(offset)
            for (lesson in lessons) {
                if (!lesson.occursOn(day)) continue
                if ((lesson.id to day) in skipped) continue
                val start = lesson.startsOn(day)
                // A schedule describes what happens from the moment it is imported.
                if (activeFrom != null && start.isBefore(activeFrom)) continue
                val trigger = start.minusMinutes(leadMinutes)
                if (!trigger.isAfter(now)) continue // already due or past: nothing to arm
                result += Reminder(Occurrence(lesson, day), trigger)
            }
        }
        return result.sortedWith(compareBy({ it.triggerAt }, { it.occurrence.lesson.id })).take(MAX_ALARMS)
    }

    /** Whether the reminder for [lesson] is due on [day] at [now] (the minute before it starts). */
    fun isDue(lesson: Lesson, day: LocalDate, now: LocalDateTime, leadMinutes: Long = LEAD_MINUTES): Boolean {
        if (!lesson.occursOn(day)) return false
        val start = lesson.startsOn(day)
        return !now.isBefore(start.minusMinutes(leadMinutes)) && now.isBefore(start)
    }
}
