package app.autopara

import android.Manifest
import android.app.NotificationManager
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.net.Uri
import android.os.Build
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.lifecycle.ViewModelProvider
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.GrantPermissionRule
import app.autopara.core.schedule.Reminders
import app.autopara.data.ThemeMode
import app.autopara.data.ViewMode
import app.autopara.notify.Notifications
import app.autopara.notify.ReminderReceiver
import app.autopara.ui.MainViewModel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TestWatcher
import org.junit.runner.Description
import org.junit.runner.RunWith
import java.io.File
import java.time.LocalDate
import java.time.LocalDateTime
import java.time.temporal.TemporalAdjusters

/**
 * Not a check: it walks through the app's main screens, in Ukrainian, and saves a picture of each.
 * `.github/workflows/android-screenshots.yml` runs it on a phone and a tablet emulator and publishes
 * the pictures to docs/android/. The normal emulator job leaves it out.
 */
@RunWith(AndroidJUnit4::class)
class ScreenshotsTest {
    @get:Rule(order = 0)
    val language = object : TestWatcher() {
        override fun starting(description: Description) {
            LocaleManager.save(ApplicationProvider.getApplicationContext(), AppLanguage.UKRAINIAN)
        }
    }

    @get:Rule(order = 1)
    val permissions: GrantPermissionRule =
        GrantPermissionRule.grant(*(if (Build.VERSION.SDK_INT >= 33) arrayOf(Manifest.permission.POST_NOTIFICATIONS) else emptyArray()))

    @get:Rule(order = 2)
    val compose = createAndroidComposeRule<MainActivity>()

    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val context: Context = ApplicationProvider.getApplicationContext()
    private val container get() = (context as AutoParaApp).container

    private fun show(text: String) = compose.onAllNodesWithText(text).fetchSemanticsNodes().isNotEmpty()

    private fun viewModel(block: (MainViewModel) -> Unit) {
        compose.activityRule.scenario.onActivity { block(ViewModelProvider(it)[MainViewModel::class.java]) }
    }

    private fun shot(name: String) {
        compose.waitForIdle()
        Thread.sleep(700) // let the last frame finish
        val bitmap = instrumentation.uiAutomation.takeScreenshot() ?: return
        // External files for `adb pull`, internal ones for `run-as` where the first is not readable.
        for (dir in listOf(File(context.getExternalFilesDir(null), "screenshots"), File(context.filesDir, "screenshots"))) {
            dir.mkdirs()
            File(dir, "$name.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
        }
    }

    @Test
    fun takeTheScreenshots() {
        runBlocking(Dispatchers.IO) { container.database.clearAllTables() }
        runBlocking { container.settings.setTheme(ThemeMode.LIGHT) }
        compose.waitUntil(15_000) { show("Імпортуйте розклад") }
        shot("01-import")

        runBlocking { instrumentation.context.assets.open("sample_timetable.docx").use { container.repository.importDocx(it) } }
        compose.waitUntil(15_000) { show("Розклад") }

        // The weekday with the most classes makes the fullest picture.
        val lessons = runBlocking { container.repository.lessons(container.settings.current().selectedGroupId!!).first() }
        val busiest = lessons.groupBy { it.dayOfWeek }.maxByOrNull { it.value.size }!!.key
        val day = LocalDate.now().with(TemporalAdjusters.nextOrSame(busiest))
        val lesson = lessons.first { it.dayOfWeek == busiest }

        viewModel { it.setViewMode(ViewMode.DAY); it.selectDate(day) }
        compose.waitUntil(15_000) { show(lesson.subject) }
        shot("02-day")

        viewModel { it.select(lesson.id, day) }
        compose.waitUntil(10_000) { show("Редагувати") }
        shot("03-details")
        viewModel { it.dismissDetail() }

        viewModel { it.setViewMode(ViewMode.WEEK) }
        compose.waitForIdle()
        shot("04-week")

        viewModel { it.setViewMode(ViewMode.MONTH) }
        compose.waitForIdle()
        shot("05-month")

        viewModel { it.setViewMode(ViewMode.DAY); it.startEdit(lesson.id, day) }
        compose.waitUntil(10_000) { show("Редагування пари") }
        shot("06-editor")
        viewModel { it.closeEditor() }
        compose.waitForIdle()

        compose.onNodeWithText("Налаштування").performClick()
        compose.waitUntil(10_000) { show("Мова") }
        shot("07-settings")

        runBlocking { container.settings.setTheme(ThemeMode.DARK) }
        compose.onNodeWithText("Розклад").performClick()
        viewModel { it.setViewMode(ViewMode.DAY); it.selectDate(day) }
        compose.waitUntil(15_000) { show(lesson.subject) }
        shot("08-day-dark")
        viewModel { it.setViewMode(ViewMode.WEEK) }
        compose.waitForIdle()
        shot("09-week-dark")

        // The reminder itself: deliver the alarm and pull the notification shade down.
        shell("appops set ${context.packageName} SCHEDULE_EXACT_ALARM allow")
        runBlocking { container.reminders.rearm() }
        val inputs = runBlocking { container.repository.alarmInputs() }
        val first = Reminders.upcoming(inputs.lessons, LocalDateTime.now(), inputs.skipped, activeFrom = inputs.activeFrom).first().occurrence
        context.sendBroadcast(
            Intent(context, ReminderReceiver::class.java)
                .setAction(ReminderReceiver.ACTION_REMIND)
                .setData(Uri.parse("autopara://remind/${first.lesson.id}/${first.date.toEpochDay()}")),
        )
        val manager = context.getSystemService(NotificationManager::class.java)
        val id = Notifications.notificationId(first.lesson.id, first.date)
        val deadline = System.currentTimeMillis() + 15_000
        while (System.currentTimeMillis() < deadline && manager.activeNotifications.none { it.id == id }) Thread.sleep(200)
        shell("cmd statusbar expand-notifications")
        Thread.sleep(1500)
        shot("10-notification")
        shell("cmd statusbar collapse")
    }

    private fun shell(command: String) {
        instrumentation.uiAutomation.executeShellCommand(command).close()
        Thread.sleep(300)
    }
}
