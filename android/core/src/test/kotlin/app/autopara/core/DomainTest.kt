package app.autopara.core

import app.autopara.core.importer.Normalize
import app.autopara.core.link.MeetingLink
import app.autopara.core.model.Lesson
import app.autopara.core.model.Provider
import app.autopara.core.model.SubjectColor
import app.autopara.core.schedule.CalendarMath
import app.autopara.core.schedule.LessonFilter
import app.autopara.core.schedule.NextUp
import app.autopara.core.schedule.Reminders
import app.autopara.core.schedule.filteredBy
import app.autopara.core.schedule.findMatch
import java.time.DayOfWeek
import java.time.LocalDate
import java.time.LocalDateTime
import java.time.LocalTime
import java.time.YearMonth
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

class DomainTest {
    // 2026-03-02 is a Monday.
    private val monday = LocalDate.of(2026, 3, 2)

    private fun lesson(
        id: Long,
        day: DayOfWeek = DayOfWeek.MONDAY,
        start: String = "09:30",
        end: String = "10:50",
        subject: String = "Психологія",
        teacher: String = "(Іваненко І.І.)",
        url: String? = "https://us02web.zoom.us/j/1",
        onDate: LocalDate? = null,
        repeatUntil: LocalDate? = null,
        skip: Set<LocalDate> = emptySet(),
    ) = Lesson(
        id = id, courseId = 1, dayOfWeek = day, pair = 2,
        start = LocalTime.parse(start), end = LocalTime.parse(end),
        subject = subject, teacher = teacher, url = url,
        provider = MeetingLink.providerOf(url), onDate = onDate, repeatUntil = repeatUntil, skipDates = skip,
    )

    // ---- Lesson.occursOn

    @Test
    fun `a weekly template occurs every week on its weekday only`() {
        val l = lesson(1)
        assertTrue(l.occursOn(monday))
        assertTrue(l.occursOn(monday.plusWeeks(5)))
        assertFalse(l.occursOn(monday.plusDays(1)))
    }

    @Test
    fun `a dated lesson occurs once and a series occurs weekly until its end`() {
        val once = lesson(1, onDate = monday)
        assertTrue(once.occursOn(monday))
        assertFalse(once.occursOn(monday.plusWeeks(1)))

        val series = lesson(2, onDate = monday, repeatUntil = monday.plusWeeks(2))
        assertTrue(series.occursOn(monday.plusWeeks(2)))
        assertFalse(series.occursOn(monday.plusWeeks(3)))
        assertFalse(series.occursOn(monday.minusWeeks(1)))
    }

    @Test
    fun `a skipped date is not an occurrence`() {
        val l = lesson(1, skip = setOf(monday.plusWeeks(1)))
        assertTrue(l.occursOn(monday))
        assertFalse(l.occursOn(monday.plusWeeks(1)))
        assertTrue(l.occursOn(monday.plusWeeks(2)))
    }

    // ---- Normalize

    @Test
    fun `day names resolve in several languages and with trailing text`() {
        assertEquals(4, Normalize.dayIndex("П`ятниця"))
        assertEquals(0, Normalize.dayIndex("Monday"))
        assertEquals(2, Normalize.dayIndex("Середа 04.09"))
        assertNull(Normalize.dayIndex("Розклад"))
    }

    @Test
    fun `times parse from bare starts and ranges`() {
        assertEquals("08:00", Normalize.parseTime("8.00"))
        assertEquals("08:00" to "09:20", Normalize.parseTimeRange("8.00-9.20"))
        assertEquals("09:30" to null, Normalize.parseTimeRange("9.30"))
        assertEquals(3, Normalize.pairFromTime("11.20"))
        assertEquals(2, Normalize.pairSlot("10:00"))
    }

    @Test
    fun `urls lose trailing punctuation`() {
        assertEquals(listOf("https://meet.google.com/abc-defg-hij"), Normalize.findUrls("join https://meet.google.com/abc-defg-hij."))
    }

    // ---- MeetingLink

    @Test
    fun `only http and https links are openable`() {
        assertTrue(MeetingLink.isOpenable(" https://zoom.us/j/1 "))
        assertFalse(MeetingLink.isOpenable("file:///etc/passwd"))
        assertFalse(MeetingLink.isOpenable("intent://scan/#Intent;scheme=zxing;end"))
        assertFalse(MeetingLink.isOpenable(null))
    }

    @Test
    fun `providers and their native packages`() {
        assertEquals(Provider.ZOOM, MeetingLink.providerOf("https://us02web.zoom.us/j/5"))
        assertEquals(Provider.GOOGLE_MEET, MeetingLink.providerOf("https://meet.google.com/aaa-bbbb-ccc"))
        assertEquals(Provider.UNKNOWN, MeetingLink.providerOf("https://example.com"))
        assertEquals(listOf("us.zoom.videomeetings"), MeetingLink.packagesFor(Provider.ZOOM))
        assertTrue("com.google.android.apps.tachyon" in MeetingLink.packagesFor(Provider.GOOGLE_MEET))
        assertTrue(MeetingLink.packagesFor(Provider.UNKNOWN).isEmpty())
    }

    // ---- LessonFilter

    @Test
    fun `search needs every word in subject or teacher in any order`() {
        val l = lesson(1, subject = "Загальна педагогіка", teacher = "(Роман Н.М.)")
        assertTrue(LessonFilter(text = "роман педаг").matches(l))
        assertTrue(LessonFilter(text = "ПЕДАГОГІКА загальна").matches(l))
        assertFalse(LessonFilter(text = "роман математика").matches(l))
    }

    @Test
    fun `apostrophe variants and subject filter are forgiving`() {
        val l = lesson(1, subject = "Основи п'яти")
        assertTrue(LessonFilter(text = "п`яти").matches(l))
        assertTrue(LessonFilter(subject = "основи п’яти").matches(l))
        assertFalse(LessonFilter(subject = "Інше").matches(l))
    }

    @Test
    fun `subjects are distinct and in Ukrainian alphabetical order`() {
        val lessons = listOf(lesson(1, subject = "Історія"), lesson(2, subject = "Арт"), lesson(3, subject = "арт"), lesson(4, subject = "Біологія"))
        assertEquals(listOf("Арт", "Біологія", "Історія"), LessonFilter.subjects(lessons))
    }

    @Test
    fun `find match walks to the nearest day with a matching class`() {
        val lessons = listOf(lesson(1, day = DayOfWeek.WEDNESDAY, subject = "Логіка"), lesson(2, subject = "Інше"))
        val flt = LessonFilter(text = "логіка")
        assertEquals(monday.plusDays(2), findMatch(lessons, flt, monday))
        assertEquals(monday.minusDays(5), findMatch(lessons, flt, monday, direction = -1))
        assertNull(findMatch(lessons, LessonFilter(text = "нема такого"), monday))
        assertEquals(1, lessons.filteredBy(flt).size)
    }

    // ---- NextUp

    @Test
    fun `next up is the ongoing class, else the next to start, else nothing`() {
        val lessons = listOf(lesson(1, start = "09:30", end = "10:50"), lesson(2, start = "13:00", end = "14:20"))
        val during = NextUp.find(lessons, LocalDateTime.of(monday, LocalTime.of(10, 0)))!!
        assertTrue(during.ongoing); assertEquals(1, during.lesson.id); assertEquals(50, during.minutes)

        val between = NextUp.find(lessons, LocalDateTime.of(monday, LocalTime.of(11, 0)))!!
        assertFalse(between.ongoing); assertEquals(2, between.lesson.id); assertEquals(120, between.minutes)
        assertFalse(between.showsCountdown)

        assertNull(NextUp.find(lessons, LocalDateTime.of(monday, LocalTime.of(15, 0))))
        assertEquals(2, NextUp.find(lessons, LocalDateTime.of(monday, LocalTime.of(10, 0)), settled = setOf(1L))!!.lesson.id)
    }

    // ---- Reminders (the one-minute-before rule)

    @Test
    fun `the reminder fires exactly one minute before the class starts`() {
        val l = lesson(1, start = "09:30")
        val reminders = Reminders.upcoming(listOf(l), LocalDateTime.of(monday, LocalTime.of(8, 0)))
        assertEquals(LocalDateTime.of(monday, LocalTime.of(9, 29)), reminders.first().triggerAt)
        assertEquals(Reminders.LEAD_MINUTES, 1L)
    }

    @Test
    fun `a reminder already past is not armed, and tomorrow's is`() {
        val l = lesson(1, start = "09:30")
        val after = LocalDateTime.of(monday, LocalTime.of(9, 29, 30))
        val reminders = Reminders.upcoming(listOf(l), after)
        assertEquals(monday.plusWeeks(1), reminders.first().occurrence.date)
    }

    @Test
    fun `skipped dates, skip_dates and unrelated weekdays are not armed`() {
        val l = lesson(1, start = "09:30", skip = setOf(monday.plusWeeks(1)))
        val now = LocalDateTime.of(monday, LocalTime.of(8, 0))
        val skipped = setOf(1L to monday)
        val dates = Reminders.upcoming(listOf(l), now, skipped, horizonDays = 14).map { it.occurrence.date }
        assertEquals(listOf(monday.plusWeeks(2)), dates)
    }

    @Test
    fun `reminders are sorted by time and capped`() {
        val many = (1L..80L).map { lesson(it, start = "09:30") } + lesson(999, start = "08:00", end = "09:20")
        val result = Reminders.upcoming(many, LocalDateTime.of(monday, LocalTime.of(7, 0)), horizonDays = 30)
        assertEquals(Reminders.MAX_ALARMS, result.size)
        assertEquals(999L, result.first().occurrence.lesson.id)
        assertEquals(result.sortedBy { it.triggerAt }.map { it.triggerAt }, result.map { it.triggerAt })
    }

    @Test
    fun `schedules ignore classes that started before the import`() {
        val l = lesson(1, start = "09:30")
        val importedAt = LocalDateTime.of(monday, LocalTime.of(12, 0))
        val first = Reminders.upcoming(listOf(l), LocalDateTime.of(monday, LocalTime.of(8, 0)), activeFrom = importedAt)
        assertEquals(monday.plusWeeks(1), first.first().occurrence.date)
    }

    @Test
    fun `is due only inside the lead window`() {
        val l = lesson(1, start = "09:30")
        assertFalse(Reminders.isDue(l, monday, LocalDateTime.of(monday, LocalTime.of(9, 28, 59))))
        assertTrue(Reminders.isDue(l, monday, LocalDateTime.of(monday, LocalTime.of(9, 29))))
        assertTrue(Reminders.isDue(l, monday, LocalDateTime.of(monday, LocalTime.of(9, 29, 59))))
        assertFalse(Reminders.isDue(l, monday, LocalDateTime.of(monday, LocalTime.of(9, 30))))
    }

    // ---- SubjectColor (expected values come from the desktop app's subject_color)

    @Test
    fun `a subject has the same colour as on the desktop`() {
        assertEquals(0x8566e6, SubjectColor.of("Психологія"))
        assertEquals(0xa8bdd6, SubjectColor.of("Загальна педагогіка"))
        assertEquals(0xa8bdd6, SubjectColor.of("Логіка"))
        assertEquals(0xec6b66, SubjectColor.of(" Історія "))
        assertEquals(SubjectColor.of("психологія"), SubjectColor.of("ПСИХОЛОГІЯ"))
    }

    // ---- CalendarMath

    @Test
    fun `a month is padded to full Monday-first weeks`() {
        val grid = CalendarMath.monthGrid(YearMonth.of(2026, 3))
        assertTrue(grid.all { it.size == 7 })
        assertTrue(grid.all { it.first().dayOfWeek == DayOfWeek.MONDAY })
        // 1 March 2026 is a Sunday, so the first week starts on Monday 23 February.
        assertEquals(LocalDate.of(2026, 2, 23), grid.first().first())
        assertEquals(LocalDate.of(2026, 4, 5), grid.last().last())
    }
}

class LessonDraftTest {
    private val monday = LocalDate.of(2026, 3, 2)

    private fun draft(
        subject: String = "Логіка", url: String = "", start: String = "09:30", end: String = "10:50",
    ) = app.autopara.core.schedule.LessonDraft(
        subject, "Іваненко І.І.", url, monday, weekly = true, LocalTime.parse(start), LocalTime.parse(end),
    )

    @Test
    fun `a draft needs a subject, an end after the start, and a link that can be opened`() {
        assertTrue(draft().isValid)
        assertEquals(listOf(app.autopara.core.schedule.LessonDraft.Problem.EMPTY_SUBJECT), draft(subject = "  ").problems())
        assertEquals(listOf(app.autopara.core.schedule.LessonDraft.Problem.END_NOT_AFTER_START), draft(end = "09:30").problems())
        assertEquals(listOf(app.autopara.core.schedule.LessonDraft.Problem.BAD_LINK), draft(url = "javascript:alert(1)").problems())
        assertTrue(draft(url = "https://meet.google.com/aaa-bbbb-ccc").isValid)
        assertTrue(draft(url = "").isValid)
    }

    @Test
    fun `teacher and link are stored the way the timetable writes them`() {
        val d = draft(url = "  https://zoom.us/j/1  ")
        assertEquals("(Іваненко І.І.)", d.storedTeacher)
        assertEquals("https://zoom.us/j/1", d.cleanUrl)
        assertEquals("", d.copy(teacher = "  ").storedTeacher)
        assertNull(draft(url = "").cleanUrl)
    }

    @Test
    fun `moving the start keeps a later end and pulls an earlier one along`() {
        assertEquals(LocalTime.parse("10:50"), draft().withStart(LocalTime.parse("09:00")).end)
        assertEquals(LocalTime.parse("12:20"), draft().withStart(LocalTime.parse("11:00")).end)
    }

    @Test
    fun `editing an existing class starts from its own values`() {
        val lesson = Lesson(
            id = 1, courseId = 1, dayOfWeek = DayOfWeek.MONDAY, pair = 2,
            start = LocalTime.parse("09:30"), end = LocalTime.parse("10:50"),
            subject = "Психологія", teacher = "(Петренко П.П.)", url = "https://zoom.us/j/9",
        )
        val d = app.autopara.core.schedule.LessonDraft.from(lesson, monday)
        assertEquals("Петренко П.П.", d.teacher)
        assertTrue(d.weekly) // a weekly template
        assertEquals(lesson.start, d.start)
        assertFalse(app.autopara.core.schedule.LessonDraft.from(lesson.copy(onDate = monday), monday).weekly)
    }
}
