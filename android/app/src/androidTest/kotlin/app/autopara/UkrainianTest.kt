package app.autopara

import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TestWatcher
import org.junit.runner.Description
import org.junit.runner.RunWith
import java.io.File

/** The app comes up in Ukrainian by default, with day, week and month on every screen. */
@RunWith(AndroidJUnit4::class)
class UkrainianTest {
    @get:Rule(order = 0)
    val reset = object : TestWatcher() {
        override fun starting(description: Description) {
            // A fresh install has no saved choice: Ukrainian is the default.
            ApplicationProvider.getApplicationContext<android.content.Context>()
                .getSharedPreferences("ui", android.content.Context.MODE_PRIVATE).edit().clear().commit()
        }
    }

    @get:Rule(order = 1)
    val compose = createAndroidComposeRule<MainActivity>()

    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val context = ApplicationProvider.getApplicationContext<AutoParaApp>()

    private fun show(text: String) = compose.onAllNodesWithText(text).fetchSemanticsNodes().isNotEmpty()

    @Test
    fun theWholeAppSpeaksUkrainianByDefault() {
        assertEquals(AppLanguage.UKRAINIAN, LocaleManager.current(context))
        runBlocking(Dispatchers.IO) { context.container.database.clearAllTables() }
        compose.waitUntil(15_000) { show("Імпортуйте розклад") }

        runBlocking {
            instrumentation.context.assets.open("sample_timetable.docx").use { context.container.repository.importDocx(it) }
        }
        compose.waitUntil(15_000) { show("Розклад") }
        // Navigation, the three view modes and the + button are all in Ukrainian.
        assertTrue(show("Налаштування"))
        assertTrue(show("День") && show("Тиждень") && show("Місяць"))
        assertTrue(compose.onAllNodesWithContentDescription("Додати пару").fetchSemanticsNodes().isNotEmpty())

        compose.onNodeWithText("Тиждень").performClick()
        compose.waitForIdle()
        compose.onNodeWithText("Місяць").performClick()
        compose.waitForIdle()
        val bitmap = instrumentation.uiAutomation.takeScreenshot()
        if (bitmap != null) {
            val dir = File(context.getExternalFilesDir(null), "screenshots").apply { mkdirs() }
            File(dir, "10-ukrainian-month.png").outputStream().use { bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
        }

        // The settings screen names the language and lets it be changed.
        compose.onNodeWithText("Налаштування").performClick()
        compose.waitUntil(10_000) { show("Мова") }
        assertTrue(show("Українська") && show("English"))
    }
}
