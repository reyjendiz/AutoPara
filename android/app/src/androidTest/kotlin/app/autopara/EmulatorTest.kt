package app.autopara

import android.Manifest
import android.app.Notification
import android.app.NotificationManager
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.net.Uri
import android.os.Build
import android.os.ParcelFileDescriptor
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.onLast
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTextReplacement
import androidx.compose.ui.test.performTouchInput
import androidx.lifecycle.ViewModelProvider
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.GrantPermissionRule
import app.autopara.AppLanguage
import app.autopara.LocaleManager
import app.autopara.core.model.Lesson
import app.autopara.core.schedule.Reminders
import app.autopara.link.LaunchResult
import app.autopara.link.MeetingLauncher
import app.autopara.notify.Notifications
import app.autopara.notify.ReminderReceiver
import app.autopara.ui.MainViewModel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.rules.TestWatcher
import org.junit.runner.Description
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.time.LocalDate
import java.time.LocalDateTime
import java.time.temporal.TemporalAdjusters

/**
 * End-to-end checks of the installed app on a real (emulated) device: the screens, the alarm, the
 * notification and the meeting-link fallbacks -- the parts the JVM unit tests cannot reach. The
 * timetable is the synthetic one the desktop tests use; the app is driven through its own
 * database, repository and view model, the same objects the UI reads.
 */
@RunWith(AndroidJUnit4::class)
class EmulatorTest {
    /** The app defaults to Ukrainian; these tests read English labels, so they ask for English first. */
    @get:Rule(order = 0)
    val language = object : TestWatcher() {
        override fun starting(description: Description) {
            LocaleManager.save(ApplicationProvider.getApplicationContext(), AppLanguage.ENGLISH)
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

    // ------------------------------------------------------------------ helpers

    private fun shell(command: String): String {
        val fd = instrumentation.uiAutomation.executeShellCommand(command)
        return ParcelFileDescriptor.AutoCloseInputStream(fd).bufferedReader().use { it.readText() }
    }

    private fun waitFor(timeoutMs: Long = 15_000, what: String = "condition", condition: () -> Boolean) {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (condition()) return
            Thread.sleep(200)
        }
        throw AssertionError("Timed out waiting for $what")
    }

    private fun screenshot(name: String) {
        compose.waitForIdle()
        val bitmap = instrumentation.uiAutomation.takeScreenshot() ?: return
        val dir = File(context.getExternalFilesDir(null), "screenshots").apply { mkdirs() }
        File(dir, "$name.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    private fun clearDatabase() = runBlocking(Dispatchers.IO) { container.database.clearAllTables() }

    private fun importSample() = runBlocking {
        val assets = instrumentation.context.assets
        assets.open("sample_timetable.docx").use { container.repository.importDocx(it) }
    }

    private fun selectedLessons(): List<Lesson> = runBlocking {
        val id = container.settings.current().selectedGroupId!!
        container.repository.lessons(id).first()
    }

    private fun viewModel(block: (MainViewModel) -> Unit) {
        compose.activityRule.scenario.onActivity { block(ViewModelProvider(it)[MainViewModel::class.java]) }
    }

    private fun showTextOnScreen(text: String) =
        compose.onAllNodesWithText(text).fetchSemanticsNodes().isNotEmpty()

    private fun upcomingReminders() = runBlocking {
        val inputs = container.repository.alarmInputs()
        Reminders.upcoming(inputs.lessons, LocalDateTime.now(), inputs.skipped, activeFrom = inputs.activeFrom)
    }

    private fun armedAlarms(): Int =
        context.getSharedPreferences("armed_alarms", Context.MODE_PRIVATE).getStringSet("armed", emptySet())!!.size

    private fun isTablet() = context.resources.configuration.screenWidthDp >= 840

    // ------------------------------------------------------------------ the tests

    @Test
    fun firstRunAsksForATimetableThenShowsIt() {
        clearDatabase()
        compose.waitUntil(15_000) { showTextOnScreen("Import your timetable") }
        screenshot("01-first-run")

        val summary = importSample()
        assertEquals(6, summary.courses)
        assertEquals(22, summary.lessons)

        val lesson = selectedLessons().first()
        val day = LocalDate.now().with(TemporalAdjusters.nextOrSame(lesson.dayOfWeek))
        viewModel { it.selectDate(day) }

        compose.waitUntil(15_000) { showTextOnScreen(lesson.subject) }
        assertFalse("the import screen should be gone", showTextOnScreen("Import your timetable"))
        assertTrue(showTextOnScreen("Schedule"))
        screenshot("02-day-view")
    }

    @Test
    fun layoutFollowsTheScreenSize() {
        clearDatabase()
        importSample()
        compose.waitUntil(15_000) { showTextOnScreen("Schedule") }
        // Day, week and month are offered on every screen size, phone or tablet.
        assertTrue(showTextOnScreen("Day") && showTextOnScreen("Week") && showTextOnScreen("Month"))
        compose.onNodeWithText("Week").performClick()
        compose.waitForIdle()
        screenshot("03-week")
        compose.onNodeWithText("Month").performClick()
        compose.waitForIdle()
        screenshot("03-month")
        if (isTablet()) {
            // A tablet keeps a permanent details pane beside the calendar.
            assertTrue(showTextOnScreen("Up next") || showTextOnScreen("Select a class to see its details"))
        }
    }

    @Test
    fun searchNarrowsTheDay() {
        clearDatabase()
        importSample()
        val lesson = selectedLessons().first()
        // A tablet opens on the week grid, which has no "nothing found" message; this checks the day list.
        viewModel {
            it.setViewMode(app.autopara.data.ViewMode.DAY)
            it.selectDate(LocalDate.now().with(TemporalAdjusters.nextOrSame(lesson.dayOfWeek)))
        }
        compose.waitUntil(15_000) { showTextOnScreen(lesson.subject) }

        compose.onNodeWithContentDescription("Search").performClick()
        compose.onNode(hasSetTextAction()).performTextInput("zzzz-no-such-class")
        compose.waitUntil(10_000) { showTextOnScreen("No classes match your search") }
        screenshot("04-search-empty")

        viewModel { it.setSearch("") }
        compose.waitUntil(10_000) { showTextOnScreen(lesson.subject) }
    }

    @Test
    fun alarmsAreArmedExactlyAndANotificationAppearsWithJoinAndSkip() {
        clearDatabase()
        importSample()
        // Android 12+ keeps exact alarms off until the user allows "Alarms & reminders".
        shell("appops set ${context.packageName} SCHEDULE_EXACT_ALARM allow")
        compose.waitForIdle()

        runBlocking { container.reminders.rearm() }
        assertTrue("exact alarms should now be allowed", container.reminders.canScheduleExact())

        val expected = upcomingReminders()
        assertTrue("the imported timetable should have upcoming reminders", expected.isNotEmpty())
        assertEquals("one alarm is armed per upcoming reminder", expected.size, armedAlarms())
        val dump = shell("dumpsys alarm")
        assertTrue("the system should list this app's alarms", dump.contains(context.packageName))

        // Every armed trigger is exactly one minute before its class starts.
        for (reminder in expected) {
            assertEquals(
                reminder.occurrence.startsAt.minusMinutes(1),
                reminder.triggerAt,
            )
        }

        // Deliver the first one as the alarm would.
        val first = expected.first().occurrence
        val lesson = first.lesson
        val day = first.date
        context.sendBroadcast(
            Intent(context, ReminderReceiver::class.java)
                .setAction(ReminderReceiver.ACTION_REMIND)
                .setData(Uri.parse("autopara://remind/${lesson.id}/${day.toEpochDay()}")),
        )

        val manager = context.getSystemService(NotificationManager::class.java)
        val id = Notifications.notificationId(lesson.id, day)
        waitFor(what = "the reminder notification") { manager.activeNotifications.any { it.id == id } }
        val notification = manager.activeNotifications.first { it.id == id }.notification
        assertEquals(lesson.subject, notification.extras.getString(Notification.EXTRA_TITLE))
        val actions = notification.actions.map { it.title.toString() }
        assertTrue("Skip action present: $actions", "Skip" in actions)
        if (!lesson.url.isNullOrBlank()) assertTrue("Join action present: $actions", "Join" in actions)
        assertEquals(
            NotificationManager.IMPORTANCE_HIGH,
            manager.getNotificationChannel(Notifications.CHANNEL_ID).importance,
        )

        // A second delivery of the same alarm must not notify again (fire-once).
        manager.cancel(id)
        context.sendBroadcast(
            Intent(context, ReminderReceiver::class.java)
                .setAction(ReminderReceiver.ACTION_REMIND)
                .setData(Uri.parse("autopara://remind/${lesson.id}/${day.toEpochDay()}")),
        )
        Thread.sleep(2_500)
        assertFalse("a repeated alarm must stay quiet", manager.activeNotifications.any { it.id == id })

        // The "Skip" action marks the date skipped and re-arms without it.
        context.sendBroadcast(
            Intent(context, ReminderReceiver::class.java)
                .setAction(Notifications.ACTION_SKIP)
                .setData(Uri.parse("autopara://skip/${lesson.id}/${day.toEpochDay()}")),
        )
        waitFor(what = "the date to be marked skipped") { runBlocking { container.repository.isSkipped(lesson.id, day) } }
        val afterSkip = runBlocking {
            val inputs = container.repository.alarmInputs()
            Reminders.upcoming(inputs.lessons, LocalDateTime.now(), inputs.skipped, activeFrom = inputs.activeFrom)
        }
        assertEquals(expected.size - 1, afterSkip.size)
        waitFor(what = "alarms re-armed without the skipped date") { armedAlarms() == afterSkip.size }
    }

    @Test
    fun meetingLinksFallBackGracefully() {
        // Not a web link at all: refused before anything is launched.
        assertEquals(LaunchResult.NotALink, MeetingLauncher.open(context, "file:///etc/passwd"))
        assertEquals(LaunchResult.NotALink, MeetingLauncher.open(context, null))

        // The emulator has neither Zoom nor Meet, so the link must end up in a browser (or, failing
        // that, the store) rather than failing -- and never inside a WebView of this app.
        val zoom = MeetingLauncher.open(context, "https://us02web.zoom.us/j/1000000001")
        assertTrue("unexpected result: $zoom", zoom is LaunchResult.OpenedInBrowser || zoom is LaunchResult.OpenedStore)
        if (zoom is LaunchResult.OpenedInBrowser) assertEquals(app.autopara.core.model.Provider.ZOOM, zoom.missingApp)
        instrumentation.uiAutomation.executeShellCommand("input keyevent KEYCODE_HOME").close()

        val meet = MeetingLauncher.open(context, "https://meet.google.com/aaa-bbbb-ccc")
        assertTrue("unexpected result: $meet", meet is LaunchResult.OpenedInBrowser || meet is LaunchResult.OpenedStore)
        instrumentation.uiAutomation.executeShellCommand("input keyevent KEYCODE_HOME").close()
    }

    // ------------------------------------------------------------------ adding and editing classes

    private fun showDay(lesson: Lesson) {
        viewModel {
            it.setViewMode(app.autopara.data.ViewMode.DAY)
            it.dismissDetail()
            it.selectDate(LocalDate.now().with(TemporalAdjusters.nextOrSame(lesson.dayOfWeek)))
        }
        compose.waitUntil(15_000) { showTextOnScreen(lesson.subject) }
        compose.waitForIdle()
    }

    private fun editorField(label: String) = compose.onNode(hasSetTextAction() and hasText(label))

    @Test
    fun theHomePlusAddsAClass() {
        clearDatabase()
        importSample()
        val anchor = selectedLessons().first()
        showDay(anchor)

        compose.onNodeWithContentDescription("Add class").performClick()
        compose.waitUntil(10_000) { showTextOnScreen("New class") }
        // Saving an empty class is refused and says why.
        compose.onNodeWithText("Save").performClick()
        compose.waitUntil(10_000) { showTextOnScreen("Enter a subject") }
        screenshot("05-editor-new")

        val name = "Added by hand ZZ"
        editorField("Subject").performTextInput(name)
        compose.onNodeWithText("Save").performClick()
        compose.waitUntil(15_000) { showTextOnScreen(name) }

        val saved = selectedLessons().firstOrNull { it.subject == name }
        assertTrue("the class should be stored", saved != null)
        assertFalse("a new class is a single date unless asked to repeat", saved!!.onDate == null)
        screenshot("06-added")
    }

    @Test
    fun aClassCanBeEditedByLongPressAndByThePencil() {
        clearDatabase()
        importSample()
        val lesson = selectedLessons().first()
        showDay(lesson)

        // Long press opens the editor with the class's own values.
        compose.onAllNodesWithText(lesson.subject).onFirst().performTouchInput { longClick() }
        compose.waitUntil(10_000) { showTextOnScreen("Edit class") }
        val renamed = lesson.subject + " (edited)"
        editorField("Subject").performTextReplacement(renamed)
        compose.onNodeWithText("Save").performClick()
        compose.waitUntil(15_000) { showTextOnScreen(renamed) }
        assertTrue(selectedLessons().any { it.id == lesson.id && it.subject == renamed })

        // The pencil in the tile's corner does the same.
        viewModel { it.dismissDetail() }
        compose.waitForIdle()
        screenshot("07-edited")
        compose.onAllNodesWithContentDescription("Edit class").onFirst().performClick()
        compose.waitUntil(10_000) { showTextOnScreen("Edit class") }
        screenshot("08-editor-edit")
        compose.onNodeWithText("Cancel").performClick()
        compose.waitUntil(10_000) { !showTextOnScreen("Cancel") }
    }

    @Test
    fun aClassCanBeDeleted() {
        clearDatabase()
        importSample()
        val groupId = runBlocking { container.settings.current().selectedGroupId!! }
        val day = LocalDate.now().plusDays(1)
        val draft = app.autopara.core.schedule.LessonDraft(
            "Delete me ZZ", "Тест Т.Т.", "", day, weekly = false,
            java.time.LocalTime.of(8, 0), java.time.LocalTime.of(9, 20),
        )
        runBlocking { container.repository.saveLesson(null, draft, groupId) }
        viewModel {
            it.setViewMode(app.autopara.data.ViewMode.DAY)
            it.selectDate(day)
        }
        compose.waitUntil(15_000) { showTextOnScreen("Delete me ZZ") }

        compose.onAllNodesWithText("Delete me ZZ").onFirst().performTouchInput { longClick() }
        compose.waitUntil(10_000) { showTextOnScreen("Edit class") }
        compose.onNodeWithText("Delete").performClick()
        compose.waitUntil(10_000) { showTextOnScreen("Delete this class?") }
        compose.onAllNodesWithText("Delete").onLast().performClick()
        compose.waitUntil(15_000) { !showTextOnScreen("Delete me ZZ") }
        assertTrue(selectedLessons().none { it.subject == "Delete me ZZ" })
    }

    @Test
    fun aClassAddedByHandSurvivesAReimport() {
        clearDatabase()
        importSample()
        val groupId = runBlocking { container.settings.current().selectedGroupId!! }
        val draft = app.autopara.core.schedule.LessonDraft(
            "Mine ZZ", "", "https://meet.google.com/aaa-bbbb-ccc", LocalDate.now().plusDays(2), weekly = true,
            java.time.LocalTime.of(10, 0), java.time.LocalTime.of(11, 20),
        )
        runBlocking { container.repository.saveLesson(null, draft, groupId) }
        assertTrue(selectedLessons().any { it.subject == "Mine ZZ" })

        importSample() // a fresh copy of the same timetable replaces the imported classes

        val after = selectedLessons()
        assertTrue("the user's own class is kept", after.any { it.subject == "Mine ZZ" })
        assertEquals("imported classes are not duplicated", 22, runBlocking {
            val ids = mutableSetOf<Long>()
            container.repository.groups.first().forEach { group ->
                container.repository.lessons(group.id).first().filter { it.subject != "Mine ZZ" }.forEach { ids += it.id }
            }
            ids.size
        })
    }

    @Test
    fun startingWithoutAFileGivesAnEmptyTimetableToAddTo() {
        clearDatabase()
        compose.waitUntil(15_000) { showTextOnScreen("Import your timetable") }
        compose.onNodeWithText("Add classes by hand").performClick()
        compose.waitUntil(15_000) { showTextOnScreen("Schedule") }
        compose.onNodeWithContentDescription("Add class").performClick()
        compose.waitUntil(10_000) { showTextOnScreen("New class") }
        screenshot("09-empty-start")
    }
}
