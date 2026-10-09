package app.autopara

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.material3.windowsizeclass.ExperimentalMaterial3WindowSizeClassApi
import androidx.compose.material3.windowsizeclass.calculateWindowSizeClass
import androidx.lifecycle.viewmodel.viewModelFactory
import androidx.lifecycle.viewmodel.initializer
import app.autopara.notify.Notifications
import app.autopara.ui.AutoParaRoot
import app.autopara.ui.MainViewModel
import java.time.LocalDate

class MainActivity : ComponentActivity() {
    private val viewModel: MainViewModel by viewModels {
        viewModelFactory {
            initializer { MainViewModel((application as AutoParaApp).container) }
        }
    }

    override fun attachBaseContext(newBase: android.content.Context) {
        super.attachBaseContext(LocaleManager.wrap(newBase))
    }

    @OptIn(ExperimentalMaterial3WindowSizeClassApi::class)
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        if (savedInstanceState == null) handleNotificationIntent(intent)
        setContent {
            // Recomputed on every size change, so rotation, split-screen and foldables just work.
            val windowSizeClass = calculateWindowSizeClass(this)
            AutoParaRoot(viewModel, windowSizeClass)
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        handleNotificationIntent(intent)
    }

    /** A tap on a reminder opens that class on its date. */
    private fun handleNotificationIntent(intent: Intent?) {
        val lessonId = intent?.getLongExtra(Notifications.EXTRA_LESSON_ID, -1L) ?: return
        val day = intent.getStringExtra(Notifications.EXTRA_DAY)?.let { runCatching { LocalDate.parse(it) }.getOrNull() }
        if (lessonId >= 0 && day != null) viewModel.showFromNotification(lessonId, day)
    }
}
