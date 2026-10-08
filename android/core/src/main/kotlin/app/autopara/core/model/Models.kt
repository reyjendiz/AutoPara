package app.autopara.core.model

import java.time.DayOfWeek
import java.time.LocalDate
import java.time.LocalDateTime
import java.time.LocalTime

/** Which meeting service a link belongs to. Decided from the URL alone. */
enum class Provider { ZOOM, GOOGLE_MEET, UNKNOWN }

/** What happened to one class on one date (the desktop app's occurrence lifecycle). */
enum class OccurrenceStatus {
    /** The reminder fired and the user was offered the link. */
    NOTIFIED,

    /** The user opened the meeting from the app or from the notification. */
    OPENED,

    /** The user dismissed this date; no reminder is shown for it. */
    SKIPPED,
}

data class Course(val id: Long, val ordinal: Int, val name: String)

data class Group(
    val id: Long,
    val courseId: Long,
    val name: String,
    val specialty: String,
)

/**
 * One class in the weekly timetable.
 *
 * A lesson without [onDate] is a weekly template: it happens every week on [dayOfWeek]. With
 * [onDate] it belongs to that date alone, or -- when [repeatUntil] is also set -- to every week
 * from [onDate] through [repeatUntil]. Mirrors `Lesson` of the desktop app so both agree on what a
 * day contains.
 */
data class Lesson(
    val id: Long,
    val courseId: Long,
    val dayOfWeek: DayOfWeek,
    val pair: Int,
    val start: LocalTime,
    val end: LocalTime,
    val subject: String,
    val teacher: String = "",
    val url: String? = null,
    val provider: Provider = Provider.UNKNOWN,
    val groupNames: List<String> = emptyList(),
    val onDate: LocalDate? = null,
    val repeatUntil: LocalDate? = null,
    val skipDates: Set<LocalDate> = emptySet(),
) {
    val needsLink: Boolean get() = url.isNullOrBlank()

    /** Whether this class takes place on [day]. The single place every view and alarm asks. */
    fun occursOn(day: LocalDate): Boolean {
        if (day.dayOfWeek != dayOfWeek) return false
        if (day in skipDates) return false
        val first = onDate ?: return true
        val last = repeatUntil ?: first
        return !day.isBefore(first) && !day.isAfter(maxOf(last, first))
    }

    fun startsOn(day: LocalDate): LocalDateTime = LocalDateTime.of(day, start)
    fun endsOn(day: LocalDate): LocalDateTime = LocalDateTime.of(day, end)
}

/** A class on a particular date: what a calendar cell or an alarm actually refers to. */
data class Occurrence(val lesson: Lesson, val date: LocalDate) {
    val startsAt: LocalDateTime get() = lesson.startsOn(date)
    val endsAt: LocalDateTime get() = lesson.endsOn(date)
}
