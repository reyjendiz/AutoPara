package app.autopara

import android.content.Context
import android.content.res.Configuration
import java.util.Locale

/** The language of the app's own text. Ukrainian unless the user picks otherwise. */
enum class AppLanguage(val tag: String?) {
    UKRAINIAN("uk"),
    ENGLISH("en"),
    SYSTEM(null),
}

/**
 * Applies the chosen language to a [Context]. It is stored in plain SharedPreferences, not in
 * DataStore, because `attachBaseContext` needs the answer synchronously before anything is drawn.
 * Activities wrap their base context with [wrap]; code that formats text from a plain application
 * context (notifications) wraps it the same way.
 */
object LocaleManager {
    private const val PREFS = "ui"
    private const val KEY = "language"

    fun current(context: Context): AppLanguage {
        val name = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getString(KEY, null)
        return AppLanguage.entries.firstOrNull { it.name == name } ?: AppLanguage.UKRAINIAN
    }

    fun save(context: Context, language: AppLanguage) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putString(KEY, language.name).apply()
    }

    /** [base] with the chosen language applied; the device language when the choice is "system". */
    fun wrap(base: Context): Context {
        val tag = current(base).tag ?: return base
        val locale = Locale.forLanguageTag(tag)
        Locale.setDefault(locale)
        val config = Configuration(base.resources.configuration)
        config.setLocale(locale)
        return base.createConfigurationContext(config)
    }
}
