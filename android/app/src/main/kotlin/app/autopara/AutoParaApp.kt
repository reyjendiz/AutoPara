package app.autopara

import android.app.Application
import android.content.Context
import app.autopara.data.AutoParaDatabase
import app.autopara.data.ScheduleRepository
import app.autopara.data.SettingsRepository
import app.autopara.notify.Notifications
import app.autopara.notify.ReminderScheduler
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import java.time.LocalDate

/** Hand-rolled dependency container: small enough that a DI framework would only add build risk. */
class AppContainer(context: Context) {
    val context: Context = context.applicationContext

    /** Outlives any screen: alarms must be (re)armed and receivers served with no UI around. */
    val appScope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    val database = AutoParaDatabase.create(context)
    val settings = SettingsRepository(context)
    val repository = ScheduleRepository(database, settings)
    val reminders = ReminderScheduler(context, repository)

    /**
     * Keeps the armed alarms in step with the data: whenever the selected group, its lessons, the
     * skipped dates or the reminders switch change, the alarms are rebuilt. Also runs once at start.
     */
    @OptIn(ExperimentalCoroutinesApi::class, FlowPreview::class)
    fun keepRemindersInSync() {
        appScope.launch {
            val prefs = settings.settings
            val lessons = prefs.map { it.selectedGroupId }.distinctUntilChanged().flatMapLatest { id ->
                if (id == null) flowOf(emptyList()) else repository.lessons(id)
            }
            combine(
                prefs.map { Triple(it.selectedGroupId, it.remindersEnabled, it.activeFrom) }.distinctUntilChanged(),
                lessons,
                repository.skipped(LocalDate.now()),
            ) { _, _, _ -> Unit }
                .debounce(300)
                .collect { reminders.rearm() }
        }
    }
}

class AutoParaApp : Application() {
    lateinit var container: AppContainer
        private set

    override fun onCreate() {
        super.onCreate()
        container = AppContainer(this)
        Notifications.ensureChannel(this)
        container.keepRemindersInSync()
    }
}
