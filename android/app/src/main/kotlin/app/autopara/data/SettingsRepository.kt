package app.autopara.data

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.longPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import java.time.LocalDateTime

enum class ThemeMode { SYSTEM, LIGHT, DARK }

enum class ViewMode { DAY, WEEK, MONTH }

data class Settings(
    val selectedGroupId: Long? = null,
    val theme: ThemeMode = ThemeMode.SYSTEM,
    val remindersEnabled: Boolean = true,
    val viewMode: ViewMode? = null, // null: choose by screen size
    /** When the timetable was imported; classes that started before it are not this app's business. */
    val activeFrom: LocalDateTime? = null,
)

private val Context.dataStore: DataStore<Preferences> by preferencesDataStore(name = "settings")

class SettingsRepository(private val context: Context) {
    private object Keys {
        val group = longPreferencesKey("selected_group_id")
        val theme = stringPreferencesKey("theme")
        val reminders = booleanPreferencesKey("reminders_enabled")
        val view = stringPreferencesKey("view_mode")
        val activeFrom = stringPreferencesKey("active_from")
    }

    val settings: Flow<Settings> = context.dataStore.data.map { prefs ->
        Settings(
            selectedGroupId = prefs[Keys.group],
            theme = prefs[Keys.theme]?.let { runCatching { ThemeMode.valueOf(it) }.getOrNull() } ?: ThemeMode.SYSTEM,
            remindersEnabled = prefs[Keys.reminders] ?: true,
            viewMode = prefs[Keys.view]?.let { runCatching { ViewMode.valueOf(it) }.getOrNull() },
            activeFrom = prefs[Keys.activeFrom]?.let { runCatching { LocalDateTime.parse(it) }.getOrNull() },
        )
    }

    suspend fun current(): Settings = settings.first()

    suspend fun selectGroup(id: Long) = context.dataStore.edit { it[Keys.group] = id }
    suspend fun setTheme(mode: ThemeMode) = context.dataStore.edit { it[Keys.theme] = mode.name }
    suspend fun setRemindersEnabled(enabled: Boolean) = context.dataStore.edit { it[Keys.reminders] = enabled }
    suspend fun setViewMode(mode: ViewMode) = context.dataStore.edit { it[Keys.view] = mode.name }
    suspend fun setActiveFrom(at: LocalDateTime) = context.dataStore.edit { it[Keys.activeFrom] = at.toString() }
}
