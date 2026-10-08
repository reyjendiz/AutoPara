package app.autopara.data

import androidx.room.withTransaction
import app.autopara.core.importer.DocxReader
import app.autopara.core.importer.ScheduleParser
import app.autopara.core.model.Course
import app.autopara.core.model.Group
import app.autopara.core.model.Lesson
import app.autopara.core.model.OccurrenceStatus
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.withContext
import java.io.InputStream
import java.time.LocalDate
import java.time.LocalDateTime

/** What an import found, for the confirmation message. */
data class ImportSummary(val courses: Int, val groups: Int, val lessons: Int, val withoutLink: Int)

/** Everything the alarm scheduler needs, read in one go. */
data class AlarmInputs(
    val lessons: List<Lesson>,
    val skipped: Set<Pair<Long, LocalDate>>,
    val activeFrom: LocalDateTime?,
    val remindersEnabled: Boolean,
)

class ScheduleRepository(
    private val db: AutoParaDatabase,
    private val settings: SettingsRepository,
) {
    private val dao = db.dao()

    val courses: Flow<List<Course>> = dao.courses().map { rows -> rows.map { it.toDomain() } }
    val groups: Flow<List<Group>> = dao.groups().map { rows -> rows.map { it.toDomain() } }

    fun lessons(groupId: Long): Flow<List<Lesson>> =
        dao.lessonsForGroup(groupId).map { rows -> rows.map { it.toDomain() } }

    suspend fun lesson(id: Long): Lesson? = dao.lesson(id)?.toDomain()

    fun statuses(from: LocalDate, to: LocalDate): Flow<Map<Pair<Long, LocalDate>, OccurrenceStatus>> =
        dao.occurrencesBetween(from.toString(), to.toString()).map { rows -> rows.toStatusMap() }

    /**
     * Replace the stored timetable with the one in [input].
     *
     * Parsing runs off the main thread and completes before the database is touched, so a document
     * that fails to parse leaves the previous timetable intact. Throws
     * [app.autopara.core.importer.DocxReader.InvalidDocumentException] for a file that is not a docx.
     */
    suspend fun importDocx(input: InputStream, now: LocalDateTime = LocalDateTime.now()): ImportSummary {
        val parsed = withContext(Dispatchers.Default) { ScheduleParser.parse(input) }
        // A document with no classes in it is the wrong file, not an empty timetable: keep the old one.
        if (parsed.sumOf { it.lessons.size } == 0) {
            throw DocxReader.InvalidDocumentException("The document contains no classes")
        }
        var groupCount = 0
        var lessonCount = 0
        var withoutLink = 0
        var firstGroup: Long? = null

        db.withTransaction {
            dao.clearAll()
            for (course in parsed) {
                val courseId = dao.insertCourse(CourseEntity(ordinal = course.ordinal, name = course.name))
                val groupIds = LinkedHashMap<String, Long>()
                for (group in course.groups) {
                    val id = dao.insertGroup(GroupEntity(courseId = courseId, name = group.name, specialty = group.specialty))
                    groupIds.putIfAbsent(group.name, id)
                    groupCount++
                    if (firstGroup == null) firstGroup = id
                }
                for (lesson in course.lessons) {
                    val lessonId = dao.insertLesson(
                        LessonEntity(
                            courseId = courseId,
                            weekday = lesson.dayIndex + 1,
                            pair = lesson.pair,
                            startTime = lesson.startTime,
                            endTime = lesson.endTime,
                            subject = lesson.subject,
                            teacher = lesson.teacher,
                            url = lesson.url,
                            provider = lesson.provider.name,
                            onDay = null,
                            repeatUntilDay = null,
                        ),
                    )
                    for (name in lesson.groupNames) groupIds[name]?.let { dao.insertLessonGroup(LessonGroupEntity(lessonId, it)) }
                    lessonCount++
                    if (lesson.url.isNullOrBlank()) withoutLink++
                }
            }
        }
        // A schedule describes what happens from the moment it is imported: earlier classes of the
        // same day are not reminded about.
        settings.setActiveFrom(now)
        firstGroup?.let { settings.selectGroup(it) }
        return ImportSummary(parsed.size, groupCount, lessonCount, withoutLink)
    }

    /** Record what happened to a class on a date; null forgets it. */
    suspend fun setStatus(lessonId: Long, day: LocalDate, status: OccurrenceStatus?) {
        if (status == null) {
            dao.clearOccurrence(lessonId, day.toString())
        } else {
            dao.setOccurrence(OccurrenceEntity(lessonId, day.toString(), status.name))
        }
    }

    /**
     * Claim the reminder for a class on a date. True only for the first caller, which is the one
     * that may notify -- a repeated or late alarm finds the row and does nothing.
     */
    suspend fun claimReminder(lessonId: Long, day: LocalDate): Boolean =
        dao.claim(OccurrenceEntity(lessonId, day.toString(), OccurrenceStatus.NOTIFIED.name)) != -1L

    suspend fun markOpened(lessonId: Long, day: LocalDate) {
        val existing = dao.occurrence(lessonId, day.toString())
        // A skipped class the user opens anyway is no longer skipped.
        if (existing == null || existing.status != OccurrenceStatus.OPENED.name) {
            dao.setOccurrence(OccurrenceEntity(lessonId, day.toString(), OccurrenceStatus.OPENED.name))
        }
    }

    suspend fun isSkipped(lessonId: Long, day: LocalDate): Boolean =
        dao.occurrence(lessonId, day.toString())?.status == OccurrenceStatus.SKIPPED.name

    fun skipped(from: LocalDate): Flow<Set<Pair<Long, LocalDate>>> =
        dao.skippedFrom(from.toString()).map { rows -> rows.map { it.lessonId to LocalDate.parse(it.day) }.toSet() }

    /** The inputs for (re)arming alarms, read from the current database state. */
    suspend fun alarmInputs(today: LocalDate = LocalDate.now()): AlarmInputs {
        val current = settings.current()
        val groupId = current.selectedGroupId
        val lessons = if (groupId == null) emptyList() else dao.lessonsForGroupNow(groupId).map { it.toDomain() }
        val skipped = skippedNow(today)
        return AlarmInputs(lessons, skipped, current.activeFrom, current.remindersEnabled)
    }

    private suspend fun skippedNow(from: LocalDate): Set<Pair<Long, LocalDate>> =
        dao.skippedFrom(from.toString()).first().map { it.lessonId to LocalDate.parse(it.day) }.toSet()

    private fun List<OccurrenceEntity>.toStatusMap(): Map<Pair<Long, LocalDate>, OccurrenceStatus> =
        associate { (it.lessonId to LocalDate.parse(it.day)) to OccurrenceStatus.valueOf(it.status) }
}
