package app.autopara.notify

import android.annotation.SuppressLint
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import app.autopara.MainActivity
import app.autopara.R
import app.autopara.core.link.MeetingLink
import app.autopara.core.model.Lesson
import app.autopara.core.model.Provider
import java.time.LocalDate
import java.time.LocalDateTime
import java.time.ZoneId
import java.time.format.DateTimeFormatter

/** Builds and shows the "class starts in a minute" notification. */
object Notifications {
    const val CHANNEL_ID = "class_reminders"

    const val ACTION_SKIP = "app.autopara.action.SKIP"
    const val EXTRA_LESSON_ID = "lesson_id"
    const val EXTRA_DAY = "day"
    const val EXTRA_URL = "url"

    private val timeFormat = DateTimeFormatter.ofPattern("HH:mm")

    fun ensureChannel(context: Context) {
        val manager = context.getSystemService(NotificationManager::class.java)
        val text = app.autopara.LocaleManager.wrap(context)
        val channel = NotificationChannel(
            CHANNEL_ID,
            text.getString(R.string.channel_name),
            NotificationManager.IMPORTANCE_HIGH,
        ).apply {
            description = text.getString(R.string.channel_description)
            enableVibration(true)
        }
        manager.createNotificationChannel(channel)
    }

    fun areEnabled(context: Context): Boolean = NotificationManagerCompat.from(context).areNotificationsEnabled()

    /** A stable id per (class, date), so a repeated post replaces rather than stacks. */
    fun notificationId(lessonId: Long, day: LocalDate): Int = (lessonId * 1_000L + day.toEpochDay() % 1_000L).toInt()

    private fun uri(kind: String, lessonId: Long, day: LocalDate): Uri = Uri.parse("autopara://$kind/$lessonId/${day.toEpochDay()}")

    @SuppressLint("MissingPermission") // guarded by areNotificationsEnabled(), which covers POST_NOTIFICATIONS
    fun postReminder(context: Context, lesson: Lesson, day: LocalDate, now: LocalDateTime) {
        if (!areEnabled(context)) return
        ensureChannel(context)
        // Strings come from the language chosen in the app; intents and the notification itself use the real context.
        val text = app.autopara.LocaleManager.wrap(context)
        val id = notificationId(lesson.id, day)
        val start = lesson.startsOn(day)
        val provider = lesson.provider.takeIf { it != Provider.UNKNOWN } ?: MeetingLink.providerOf(lesson.url)
        val providerName = when (provider) {
            Provider.ZOOM -> text.getString(R.string.provider_zoom)
            Provider.GOOGLE_MEET -> text.getString(R.string.provider_meet)
            Provider.UNKNOWN -> text.getString(R.string.provider_link)
        }
        val hasLink = MeetingLink.isOpenable(lesson.url)
        val whenText = if (now.isBefore(start)) {
            text.getString(R.string.notif_starts_soon, start.format(timeFormat))
        } else {
            text.getString(R.string.notif_started, start.format(timeFormat))
        }
        val detail = if (hasLink) "$whenText · $providerName" else "$whenText · ${text.getString(R.string.notif_no_link)}"

        val open = PendingIntent.getActivity(
            context, id,
            Intent(context, MainActivity::class.java)
                .setData(uri("lesson", lesson.id, day))
                .putExtra(EXTRA_LESSON_ID, lesson.id)
                .putExtra(EXTRA_DAY, day.toString())
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )

        val builder = NotificationCompat.Builder(context, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_stat_class)
            .setContentTitle(lesson.subject)
            .setContentText(detail)
            .setStyle(NotificationCompat.BigTextStyle().bigText(listOf(detail, lesson.teacher.trim('(', ')')).filter { it.isNotBlank() }.joinToString("\n")))
            .setCategory(NotificationCompat.CATEGORY_REMINDER)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
            .setWhen(start.atZone(ZoneId.systemDefault()).toInstant().toEpochMilli())
            .setShowWhen(true)
            .setAutoCancel(true)
            .setContentIntent(open)

        if (hasLink) {
            // An Activity (not a receiver or service) so Android 12's notification-trampoline
            // restriction does not apply: it opens the meeting and finishes at once.
            val join = PendingIntent.getActivity(
                context, id,
                Intent(context, JoinActivity::class.java)
                    .setData(uri("join", lesson.id, day))
                    .putExtra(EXTRA_LESSON_ID, lesson.id)
                    .putExtra(EXTRA_DAY, day.toString())
                    .putExtra(EXTRA_URL, lesson.url),
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
            )
            builder.addAction(0, text.getString(R.string.notif_join), join)
        }
        val skip = PendingIntent.getBroadcast(
            context, id,
            Intent(context, ReminderReceiver::class.java)
                .setAction(ACTION_SKIP)
                .setData(uri("skip", lesson.id, day)),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        builder.addAction(0, text.getString(R.string.notif_skip), skip)

        NotificationManagerCompat.from(context).notify(id, builder.build())
    }

    fun cancel(context: Context, lessonId: Long, day: LocalDate) {
        NotificationManagerCompat.from(context).cancel(notificationId(lessonId, day))
    }
}
