package app.autopara.notify

import android.app.Activity
import android.os.Bundle
import android.widget.Toast
import app.autopara.AutoParaApp
import app.autopara.R
import app.autopara.link.LaunchResult
import app.autopara.link.MeetingLauncher
import kotlinx.coroutines.launch
import java.time.LocalDate

/**
 * The "Join" button of a reminder. It has no UI: it hands the meeting link to the native app (see
 * [MeetingLauncher]) and finishes immediately. It is an Activity because Android 12+ forbids
 * notification actions from starting activities through a receiver or service.
 */
class JoinActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val url = intent.getStringExtra(Notifications.EXTRA_URL)
        val lessonId = intent.getLongExtra(Notifications.EXTRA_LESSON_ID, -1L)
        val day = intent.getStringExtra(Notifications.EXTRA_DAY)?.let { runCatching { LocalDate.parse(it) }.getOrNull() }

        val result = MeetingLauncher.open(this, url)
        when (result) {
            is LaunchResult.OpenedInBrowser ->
                if (result.missingApp != null) Toast.makeText(this, R.string.app_missing_opened_browser, Toast.LENGTH_LONG).show()
            LaunchResult.Failed, LaunchResult.NotALink ->
                Toast.makeText(this, R.string.link_could_not_open, Toast.LENGTH_LONG).show()
            else -> Unit
        }

        if (lessonId >= 0 && day != null && (result is LaunchResult.OpenedInApp || result is LaunchResult.OpenedInBrowser)) {
            val container = (applicationContext as AutoParaApp).container
            container.appScope.launch { container.repository.markOpened(lessonId, day) }
            Notifications.cancel(this, lessonId, day)
        }
        finish()
    }
}
