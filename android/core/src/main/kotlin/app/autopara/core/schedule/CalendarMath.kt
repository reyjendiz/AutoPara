package app.autopara.core.schedule

import app.autopara.core.model.Lesson
import app.autopara.core.model.Occurrence
import java.time.DayOfWeek
import java.time.LocalDate
import java.time.YearMonth
import java.time.temporal.TemporalAdjusters

/** Date arithmetic the calendar views share. Weeks start on Monday, as in the timetable. */
object CalendarMath {
    fun weekStart(day: LocalDate): LocalDate = day.with(TemporalAdjusters.previousOrSame(DayOfWeek.MONDAY))

    fun weekDays(day: LocalDate): List<LocalDate> = weekStart(day).let { monday -> (0L..6L).map { monday.plusDays(it) } }

    /** The month as full Monday-first weeks, padded with the neighbouring months' days. */
    fun monthGrid(month: YearMonth): List<List<LocalDate>> {
        val first = weekStart(month.atDay(1))
        val weeks = ArrayList<List<LocalDate>>()
        var monday = first
        while (!monday.isAfter(month.atEndOfMonth())) {
            weeks += (0L..6L).map { monday.plusDays(it) }
            monday = monday.plusWeeks(1)
        }
        return weeks
    }

    /** The classes happening on [day], in time order. */
    fun occurrencesOn(lessons: List<Lesson>, day: LocalDate): List<Occurrence> =
        lessons.filter { it.occursOn(day) }
            .sortedWith(compareBy({ it.start }, { it.id }))
            .map { Occurrence(it, day) }
}
