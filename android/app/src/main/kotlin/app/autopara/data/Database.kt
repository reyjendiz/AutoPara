package app.autopara.data

import android.content.Context
import androidx.room.Dao
import androidx.room.Database
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import androidx.room.Room
import androidx.room.RoomDatabase
import androidx.room.Transaction
import androidx.room.Upsert
import kotlinx.coroutines.flow.Flow

@Dao
interface ScheduleDao {
    @Query("SELECT * FROM courses ORDER BY ordinal")
    fun courses(): Flow<List<CourseEntity>>

    @Query("SELECT * FROM study_groups ORDER BY courseId, id")
    fun groups(): Flow<List<GroupEntity>>

    @Transaction
    @Query(
        "SELECT l.* FROM lessons l INNER JOIN lesson_groups lg ON lg.lessonId = l.id " +
            "WHERE lg.groupId = :groupId ORDER BY l.weekday, l.startTime, l.id",
    )
    fun lessonsForGroup(groupId: Long): Flow<List<LessonWithGroups>>

    @Transaction
    @Query(
        "SELECT l.* FROM lessons l INNER JOIN lesson_groups lg ON lg.lessonId = l.id " +
            "WHERE lg.groupId = :groupId ORDER BY l.weekday, l.startTime, l.id",
    )
    suspend fun lessonsForGroupNow(groupId: Long): List<LessonWithGroups>

    @Transaction
    @Query("SELECT * FROM lessons WHERE id = :id")
    suspend fun lesson(id: Long): LessonWithGroups?

    @Query("SELECT COUNT(*) FROM lessons")
    suspend fun lessonCount(): Int

    // ---- import

    @Query("DELETE FROM courses")
    suspend fun clearAll()

    @Insert
    suspend fun insertCourse(course: CourseEntity): Long

    @Insert
    suspend fun insertGroup(group: GroupEntity): Long

    @Insert
    suspend fun insertLesson(lesson: LessonEntity): Long

    @Insert(onConflict = OnConflictStrategy.IGNORE)
    suspend fun insertLessonGroup(ref: LessonGroupEntity)

    // ---- occurrences

    /** Returns -1 when the occurrence already has a row: the caller lost the claim. */
    @Insert(onConflict = OnConflictStrategy.IGNORE)
    suspend fun claim(occurrence: OccurrenceEntity): Long

    @Upsert
    suspend fun setOccurrence(occurrence: OccurrenceEntity)

    @Query("DELETE FROM occurrences WHERE lessonId = :lessonId AND day = :day")
    suspend fun clearOccurrence(lessonId: Long, day: String)

    @Query("SELECT * FROM occurrences WHERE day >= :from AND day <= :to")
    fun occurrencesBetween(from: String, to: String): Flow<List<OccurrenceEntity>>

    @Query("SELECT * FROM occurrences WHERE status = 'SKIPPED' AND day >= :from")
    fun skippedFrom(from: String): Flow<List<OccurrenceEntity>>

    @Query("SELECT * FROM occurrences WHERE lessonId = :lessonId AND day = :day")
    suspend fun occurrence(lessonId: Long, day: String): OccurrenceEntity?
}

@Database(
    entities = [
        CourseEntity::class, GroupEntity::class, LessonEntity::class,
        LessonGroupEntity::class, OccurrenceEntity::class,
    ],
    version = 1,
    exportSchema = false,
)
abstract class AutoParaDatabase : RoomDatabase() {
    abstract fun dao(): ScheduleDao

    companion object {
        fun create(context: Context): AutoParaDatabase =
            Room.databaseBuilder(context.applicationContext, AutoParaDatabase::class.java, "autopara.db").build()
    }
}
