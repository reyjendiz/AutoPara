package app.autopara.notify

import android.app.AlarmManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import app.autopara.core.schedule.Reminders
import app.autopara.data.ScheduleRepository
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import java.time.LocalDate
import java.time.LocalDateTime
import java.time.ZoneId

/**
 * Arms one exact alarm per upcoming class, one minute before it starts.
 *
 * Why `AlarmManager` and not WorkManager: WorkManager is deferrable and may run minutes late, which
 * defeats a "1 minute before" reminder. `setExactAndAllowWhileIdle` fires on the minute even in
 * Doze. Alarms are armed for the next [Reminders.HORIZON_DAYS] days and re-armed whenever the
 * timetable or settings change, when an alarm fires, and after reboot, a clock change or an app
 * update (see [RescheduleReceiver]) -- alarms do not survive a reboot on their own.
 */
class ReminderScheduler(
    private val context: Context,
    private val repository: ScheduleRepository,
) {
    private val alarmManager = context.getSystemService(AlarmManager::class.java)
    private val prefs = context.getSharedPreferences("armed_alarms", Context.MODE_PRIVATE)
    private val mutex = Mutex()

    /** Android 12+ lets the user revoke exact alarms; before that they are always allowed. */
    fun canScheduleExact(): Boolean = Build.VERSION.SDK_INT < Build.VERSION_CODES.S || alarmManager.canScheduleExactAlarms()

    /** Cancels every alarm this app armed and arms the current set. Safe to call from anywhere. */
    suspend fun rearm(now: LocalDateTime = LocalDateTime.now()) = mutex.withLock {
        cancelArmed()
        val inputs = repository.alarmInputs(now.toLocalDate())
        if (!inputs.remindersEnabled) return@withLock

        val reminders = Reminders.upcoming(
            lessons = inputs.lessons,
            now = now,
            skipped = inputs.skipped,
            activeFrom = inputs.activeFrom,
        )
        val exact = canScheduleExact()
        val armed = HashSet<String>()
        for (reminder in reminders) {
            val lesson = reminder.occurrence.lesson
            val day = reminder.occurrence.date
            val triggerAt = reminder.triggerAt.atZone(ZoneId.systemDefault()).toInstant().toEpochMilli()
            arm(lesson.id, day, triggerAt, exact)
            armed += key(lesson.id, day)
        }
        prefs.edit().putStringSet(ARMED_KEY, armed).apply()
    }

    private fun arm(lessonId: Long, day: LocalDate, triggerAtMillis: Long, exact: Boolean) {
        val pending = pendingIntent(lessonId, day, create = true) ?: return
        if (exact) {
            try {
                alarmManager.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, triggerAtMillis, pending)
                return
            } catch (_: SecurityException) {
                // Permission revoked between the check and the call: fall through to the inexact alarm.
            }
        }
        alarmManager.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, triggerAtMillis, pending)
    }

    private fun cancelArmed() {
        for (entry in prefs.getStringSet(ARMED_KEY, emptySet()).orEmpty()) {
            val (lessonId, epochDay) = entry.split("/").let { it[0].toLong() to it[1].toLong() }
            pendingIntent(lessonId, LocalDate.ofEpochDay(epochDay), create = false)?.let {
                alarmManager.cancel(it)
                it.cancel()
            }
        }
        prefs.edit().remove(ARMED_KEY).apply()
    }

    /**
     * Identity of an alarm is its Intent's data URI, so the same (class, date) always maps to the
     * same PendingIntent and can be cancelled without remembering any request codes.
     */
    private fun pendingIntent(lessonId: Long, day: LocalDate, create: Boolean): PendingIntent? {
        val intent = Intent(context, ReminderReceiver::class.java)
            .setAction(ReminderReceiver.ACTION_REMIND)
            .setData(Uri.parse("autopara://remind/$lessonId/${day.toEpochDay()}"))
        val flags = PendingIntent.FLAG_IMMUTABLE or
            if (create) PendingIntent.FLAG_UPDATE_CURRENT else PendingIntent.FLAG_NO_CREATE
        return PendingIntent.getBroadcast(context, 0, intent, flags)
    }

    private fun key(lessonId: Long, day: LocalDate) = "$lessonId/${day.toEpochDay()}"

    private companion object {
        const val ARMED_KEY = "armed"
    }
}
