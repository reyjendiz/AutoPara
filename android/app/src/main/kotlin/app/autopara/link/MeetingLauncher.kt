package app.autopara.link

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri
import app.autopara.core.link.MeetingLink
import app.autopara.core.model.Provider

/** How a meeting link ended up being opened. */
sealed interface LaunchResult {
    /** The native Zoom or Meet app took it. */
    data object OpenedInApp : LaunchResult

    /** No native app handled it, so the browser did. [missingApp] is the service whose app is absent. */
    data class OpenedInBrowser(val missingApp: Provider?) : LaunchResult

    /** Nothing could open the link, so the app's Play Store page was opened instead. */
    data class OpenedStore(val provider: Provider) : LaunchResult

    /** The link is empty or not an http(s) URL. */
    data object NotALink : LaunchResult

    data object Failed : LaunchResult
}

/**
 * Opens a Zoom or Google Meet link in the native app -- never in a WebView -- and degrades
 * gracefully: native app, then the browser, then the app's store page.
 *
 * Zoom and Meet both register verified HTTPS App Links, so a plain `ACTION_VIEW` of the meeting URL
 * addressed to the app's package lands inside the app. Package visibility (Android 11+) needs the
 * `<queries>` block in the manifest for these package names.
 */
object MeetingLauncher {
    fun open(context: Context, url: String?, provider: Provider = MeetingLink.providerOf(url)): LaunchResult {
        if (!MeetingLink.isOpenable(url)) return LaunchResult.NotALink
        val uri = Uri.parse(url!!.trim())

        // 1. The native app, if one is installed and accepts the link.
        for (pkg in MeetingLink.packagesFor(provider)) {
            val intent = viewIntent(uri).setPackage(pkg)
            if (start(context, intent)) return LaunchResult.OpenedInApp
        }

        // 2. Any browser (or other handler). If the provider is known its app is evidently missing.
        val missing = provider.takeIf { it != Provider.UNKNOWN }
        if (start(context, viewIntent(uri))) return LaunchResult.OpenedInBrowser(missing)

        // 3. No browser either: offer the app from the store.
        if (missing != null && openStore(context, missing)) return LaunchResult.OpenedStore(missing)
        return LaunchResult.Failed
    }

    /** The store page of the app for [provider]: Play Store if present, its website otherwise. */
    fun openStore(context: Context, provider: Provider): Boolean {
        val pkg = MeetingLink.storePackage(provider) ?: return false
        val market = Intent(Intent.ACTION_VIEW, Uri.parse("market://details?id=$pkg"))
            .setPackage("com.android.vending")
        if (start(context, market)) return true
        return start(context, Intent(Intent.ACTION_VIEW, Uri.parse("https://play.google.com/store/apps/details?id=$pkg")))
    }

    private fun viewIntent(uri: Uri): Intent =
        Intent(Intent.ACTION_VIEW, uri).addCategory(Intent.CATEGORY_BROWSABLE)

    /** Starts [intent]; false when nothing can handle it. Safe from any Context. */
    private fun start(context: Context, intent: Intent): Boolean = try {
        context.startActivity(intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        true
    } catch (_: ActivityNotFoundException) {
        false
    } catch (_: SecurityException) {
        false
    }
}
