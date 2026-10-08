package app.autopara.core.link

import app.autopara.core.model.Provider

/**
 * Classifies a meeting URL and decides which native apps may handle it. Pure, so the rules are unit
 * tested; the Android side only turns the answer into an Intent.
 */
object MeetingLink {
    const val ZOOM_PACKAGE = "us.zoom.videomeetings"

    /** Google Meet ships as the Meet app (tachyon); the older standalone id is kept as a fallback. */
    val MEET_PACKAGES = listOf("com.google.android.apps.tachyon", "com.google.android.apps.meetings")

    /** Only ordinary web URLs are opened: the text comes from a document, so `file:`/`intent:` are refused. */
    fun isOpenable(url: String?): Boolean {
        val lowered = url?.trim()?.lowercase() ?: return false
        return lowered.startsWith("https://") || lowered.startsWith("http://")
    }

    fun providerOf(url: String?): Provider {
        val lowered = url?.lowercase() ?: return Provider.UNKNOWN
        return when {
            "meet.google.com" in lowered -> Provider.GOOGLE_MEET
            "zoom.us" in lowered || "zoom.com" in lowered -> Provider.ZOOM
            else -> Provider.UNKNOWN
        }
    }

    /** Native packages to try, most specific first. Empty for a link no known app handles. */
    fun packagesFor(provider: Provider): List<String> = when (provider) {
        Provider.ZOOM -> listOf(ZOOM_PACKAGE)
        Provider.GOOGLE_MEET -> MEET_PACKAGES
        Provider.UNKNOWN -> emptyList()
    }

    /** Where to install the app from when it is missing. */
    fun storePackage(provider: Provider): String? = packagesFor(provider).firstOrNull()
}
