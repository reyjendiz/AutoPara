package app.autopara.notify

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import app.autopara.AutoParaApp
import app.autopara.core.model.OccurrenceStatus
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeout
import java.time.LocalDate
import java.time.LocalDateTime

/**
 * Receives the exact alarm (and the notification's "skip" action), even when the app is in the
 * background or closed: the manifest registers it, so the system starts the process for it.
 */
class ReminderReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val action = intent.action
        if (action != ACTION_REMIND && action != Notifications.ACTION_SKIP) return
        val (lessonId, day) = parse(intent) ?: return

        // Database work is suspending: keep the receiver alive until it is done, but not for long.
        val pending = goAsync()
        val container = (context.applicationContext as AutoParaApp).container
        container.appScope.launch {
            try {
                withTimeout(HANDLE_TIMEOUT_MS) {
                    if (action == ACTION_REMIND) {
                        remind(context, container, lessonId, day)
                    } else {
                        container.repository.setStatus(lessonId, day, OccurrenceStatus.SKIPPED)
                        Notifications.cancel(context, lessonId, day)
                    }
                    // Whatever just happened, keep the rolling window of alarms topped up.
                    container.reminders.rearm()
                }
            } finally {
                pending.finish()
            }
        }
    }

    private suspend fun remind(context: Context, container: app.autopara.AppContainer, lessonId: Long, day: LocalDate) {
        if (!container.settings.current().remindersEnabled) return
        val lesson = container.repository.lesson(lessonId) ?: return
        if (!lesson.occursOn(day)) return
        val now = LocalDateTime.now()
        if (!now.isBefore(lesson.endsOn(day))) return // woken too late to be useful
        // Fire-once: the first caller to claim the (class, date) row notifies; a duplicate or a
        // dismissed ("skipped") date finds the row and stays quiet.
        if (!container.repository.claimReminder(lessonId, day)) return
        Notifications.postReminder(context, lesson, day, now)
    }

    private fun parse(intent: Intent): Pair<Long, LocalDate>? {
        // autopara://<kind>/<lessonId>/<epochDay>
        val segments = intent.data?.pathSegments ?: return null
        if (segments.size < 2) return null
        val lessonId = segments[0].toLongOrNull() ?: return null
        val epochDay = segments[1].toLongOrNull() ?: return null
        return lessonId to LocalDate.ofEpochDay(epochDay)
    }

    companion object {
        const val ACTION_REMIND = "app.autopara.action.REMIND"
        private const val HANDLE_TIMEOUT_MS = 8_000L
    }
}
