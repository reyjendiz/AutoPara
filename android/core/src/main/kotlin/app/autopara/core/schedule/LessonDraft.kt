package app.autopara.core.schedule

import app.autopara.core.link.MeetingLink
import app.autopara.core.model.Lesson
import java.time.LocalDate
import java.time.LocalTime

/**
 * What the class editor holds while the user types: a class that has not been saved yet.
 *
 * A *weekly* draft becomes a weekly template (it happens every week on the weekday of [date]); a
 * one-time draft belongs to [date] alone. Validation lives here, not in the screen, so the rules are
 * the same wherever a class is created and are covered by plain unit tests.
 */
data class LessonDraft(
    val subject: String,
    /** The teacher's name as typed, without the parentheses a timetable writes around it. */
    val teacher: String,
    val url: String,
    val date: LocalDate,
    val weekly: Boolean,
    val start: LocalTime,
    val end: LocalTime,
) {
    enum class Problem { EMPTY_SUBJECT, END_NOT_AFTER_START, BAD_LINK }

    fun problems(): List<Problem> = buildList {
        if (subject.isBlank()) add(Problem.EMPTY_SUBJECT)
        if (!end.isAfter(start)) add(Problem.END_NOT_AFTER_START)
        // A link is optional, but one that is there must be something the app will actually open.
        if (url.isNotBlank() && !MeetingLink.isOpenable(url)) add(Problem.BAD_LINK)
    }

    val isValid: Boolean get() = problems().isEmpty()

    /** The link to store: trimmed, or null when the field was left empty. */
    val cleanUrl: String? get() = url.trim().ifEmpty { null }

    /** The teacher as the timetable writes it -- in parentheses -- or empty. */
    val storedTeacher: String get() = teacher.trim().let { if (it.isEmpty()) "" else "($it)" }

    /** A new start time moves the end with it when the end would no longer be after it. */
    fun withStart(newStart: LocalTime): LessonDraft =
        copy(start = newStart, end = if (end.isAfter(newStart)) end else newStart.plusMinutes(DEFAULT_MINUTES))

    companion object {
        /** The length of a university pair. */
        const val DEFAULT_MINUTES = 80L

        fun blank(date: LocalDate): LessonDraft = LessonDraft(
            subject = "", teacher = "", url = "", date = date, weekly = false,
            start = LocalTime.of(8, 0), end = LocalTime.of(8, 0).plusMinutes(DEFAULT_MINUTES),
        )

        /** An existing class opened for editing on [date], the day it was tapped on. */
        fun from(lesson: Lesson, date: LocalDate): LessonDraft {
            val first = lesson.onDate
            val weekly = first == null || (lesson.repeatUntil != null && lesson.repeatUntil > first)
            return LessonDraft(
                subject = lesson.subject,
                teacher = lesson.teacher.trim().removePrefix("(").removeSuffix(")").trim(),
                url = lesson.url.orEmpty(),
                date = date,
                weekly = weekly,
                start = lesson.start,
                end = lesson.end,
            )
        }
    }
}
