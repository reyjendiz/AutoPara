package app.autopara.data

import androidx.room.Embedded
import androidx.room.Entity
import androidx.room.ForeignKey
import androidx.room.Index
import androidx.room.Junction
import androidx.room.PrimaryKey
import androidx.room.Relation
import app.autopara.core.model.Course
import app.autopara.core.model.Group
import app.autopara.core.model.Lesson
import app.autopara.core.model.Provider
import java.time.DayOfWeek
import java.time.LocalDate
import java.time.LocalTime

// Table and column names avoid SQL keywords ("groups", "end", "date") so raw queries need no quoting.

@Entity(tableName = "courses")
data class CourseEntity(
    @PrimaryKey(autoGenerate = true) val id: Long = 0,
    val ordinal: Int,
    val name: String,
)

@Entity(
    tableName = "study_groups",
    foreignKeys = [ForeignKey(entity = CourseEntity::class, parentColumns = ["id"], childColumns = ["courseId"], onDelete = ForeignKey.CASCADE)],
    indices = [Index("courseId")],
)
data class GroupEntity(
    @PrimaryKey(autoGenerate = true) val id: Long = 0,
    val courseId: Long,
    val name: String,
    val specialty: String,
)

@Entity(
    tableName = "lessons",
    foreignKeys = [ForeignKey(entity = CourseEntity::class, parentColumns = ["id"], childColumns = ["courseId"], onDelete = ForeignKey.CASCADE)],
    indices = [Index("courseId")],
)
data class LessonEntity(
    @PrimaryKey(autoGenerate = true) val id: Long = 0,
    val courseId: Long,
    /** ISO day of week: 1 = Monday .. 7 = Sunday. */
    val weekday: Int,
    val pair: Int,
    val startTime: String, // HH:MM
    val endTime: String,   // HH:MM
    val subject: String,
    val teacher: String,
    val url: String?,
    val provider: String,
    /** ISO dates; null for a weekly template. */
    val onDay: String?,
    val repeatUntilDay: String?,
    /** Added or created by the user rather than read from the timetable: kept across a re-import. */
    val isManual: Boolean = false,
)

/** Which groups attend a lesson: a class shared by two groups has two rows. */
@Entity(
    tableName = "lesson_groups",
    primaryKeys = ["lessonId", "groupId"],
    foreignKeys = [
        ForeignKey(entity = LessonEntity::class, parentColumns = ["id"], childColumns = ["lessonId"], onDelete = ForeignKey.CASCADE),
        ForeignKey(entity = GroupEntity::class, parentColumns = ["id"], childColumns = ["groupId"], onDelete = ForeignKey.CASCADE),
    ],
    indices = [Index("groupId")],
)
data class LessonGroupEntity(val lessonId: Long, val groupId: Long)

/**
 * What happened to a class on one date. The (lessonId, day) primary key is the fire-once guarantee:
 * claiming an occurrence is an INSERT that fails if the row exists, so a duplicate alarm cannot
 * produce a second notification.
 */
@Entity(
    tableName = "occurrences",
    primaryKeys = ["lessonId", "day"],
    foreignKeys = [ForeignKey(entity = LessonEntity::class, parentColumns = ["id"], childColumns = ["lessonId"], onDelete = ForeignKey.CASCADE)],
)
data class OccurrenceEntity(val lessonId: Long, val day: String, val status: String)

data class LessonWithGroups(
    @Embedded val lesson: LessonEntity,
    @Relation(
        parentColumn = "id",
        entityColumn = "id",
        associateBy = Junction(LessonGroupEntity::class, parentColumn = "lessonId", entityColumn = "groupId"),
    )
    val groups: List<GroupEntity>,
)

fun CourseEntity.toDomain() = Course(id, ordinal, name)

fun GroupEntity.toDomain() = Group(id, courseId, name, specialty)

fun LessonWithGroups.toDomain(): Lesson = Lesson(
    id = lesson.id,
    courseId = lesson.courseId,
    dayOfWeek = DayOfWeek.of(lesson.weekday),
    pair = lesson.pair,
    start = LocalTime.parse(lesson.startTime),
    end = LocalTime.parse(lesson.endTime),
    subject = lesson.subject,
    teacher = lesson.teacher,
    url = lesson.url,
    provider = runCatching { Provider.valueOf(lesson.provider) }.getOrDefault(Provider.UNKNOWN),
    groupNames = groups.map { it.name },
    onDate = lesson.onDay?.let(LocalDate::parse),
    repeatUntil = lesson.repeatUntilDay?.let(LocalDate::parse),
)
