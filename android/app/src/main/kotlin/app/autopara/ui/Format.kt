package app.autopara.ui

import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.res.stringResource
import app.autopara.R
import app.autopara.core.model.Lesson
import app.autopara.core.schedule.NextUp
import app.autopara.data.ViewMode
import java.time.DayOfWeek
import java.time.LocalDate
import java.time.LocalTime
import java.time.YearMonth
import java.time.format.DateTimeFormatter
import java.time.format.TextStyle
import java.util.Locale

/** The device language, so dates read the way the user's phone reads them. */
@Composable
fun currentLocale(): Locale = LocalConfiguration.current.locales[0] ?: Locale.getDefault()

private val clockFormat: DateTimeFormatter = DateTimeFormatter.ofPattern("HH:mm", Locale.ROOT)

fun LocalTime.clock(): String = format(clockFormat)

fun Lesson.timeRange(): String = "${start.clock()}–${end.clock()}"

private fun String.capitalised(locale: Locale): String =
    replaceFirstChar { if (it.isLowerCase()) it.titlecase(locale) else it.toString() }

fun dayTitle(date: LocalDate, locale: Locale): String =
    date.format(DateTimeFormatter.ofPattern("EEEE, d MMMM", locale)).capitalised(locale)

fun monthTitle(month: YearMonth, locale: Locale): String =
    month.format(DateTimeFormatter.ofPattern("LLLL yyyy", locale)).capitalised(locale)

fun weekTitle(monday: LocalDate, locale: Locale): String {
    val sunday = monday.plusDays(6)
    val short = DateTimeFormatter.ofPattern("d MMM", locale)
    val long = DateTimeFormatter.ofPattern("d MMM yyyy", locale)
    return "${monday.format(short)} – ${sunday.format(long)}"
}

fun periodTitle(mode: ViewMode, date: LocalDate, monday: LocalDate, locale: Locale): String = when (mode) {
    ViewMode.DAY -> dayTitle(date, locale)
    ViewMode.WEEK -> weekTitle(monday, locale)
    ViewMode.MONTH -> monthTitle(YearMonth.from(date), locale)
}

fun shortWeekday(day: DayOfWeek, locale: Locale): String =
    day.getDisplayName(TextStyle.SHORT, locale).capitalised(locale)

fun dayNumber(date: LocalDate): String = date.dayOfMonth.toString()

/** "Now · Psychology · 40 min left" / "Next · Psychology · in 25 min" / "… · at 14:40". */
@Composable
fun nextUpText(item: NextUp): String = when {
    item.ongoing -> stringResource(R.string.next_up_ongoing, item.lesson.subject, item.minutes)
    item.showsCountdown -> stringResource(R.string.next_up_in, item.lesson.subject, item.minutes)
    else -> stringResource(R.string.next_up_at, item.lesson.subject, item.lesson.start.clock())
}
