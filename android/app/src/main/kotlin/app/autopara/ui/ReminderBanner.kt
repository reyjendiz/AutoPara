package app.autopara.ui

import android.Manifest
import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.core.app.ActivityCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import app.autopara.AutoParaApp
import app.autopara.R
import app.autopara.notify.Notifications

private tailrec fun Context.findActivity(): Activity? = when (this) {
    is Activity -> this
    is ContextWrapper -> baseContext.findActivity()
    else -> null
}

/**
 * Tells the user when reminders cannot work and offers the one-tap fix: notifications switched off
 * (Android 13+ asks at runtime) or exact alarms not allowed (a system setting from Android 12).
 * Re-checks every time the app comes back to the foreground, so it vanishes once the user has
 * granted the permission in system settings.
 */
@Composable
fun ReminderBanner(enabled: Boolean, modifier: Modifier = Modifier) {
    if (!enabled) return
    val context = LocalContext.current
    val container = (context.applicationContext as AutoParaApp).container
    var tick by remember { mutableIntStateOf(0) }
    LifecycleEventEffect(Lifecycle.Event.ON_RESUME) { tick++ }

    val notificationsOk = remember(tick) { Notifications.areEnabled(context) }
    val exactOk = remember(tick) { container.reminders.canScheduleExact() }
    var asked by rememberSaveable { mutableStateOf(false) }

    val permissionLauncher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { tick++ }

    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (!notificationsOk) {
            BannerCard(
                title = stringResource(R.string.perm_notifications_title),
                body = stringResource(R.string.perm_notifications_body),
                action = stringResource(R.string.perm_allow),
            ) {
                val activity = context.findActivity()
                val canAsk = Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
                    (!asked || (activity != null && ActivityCompat.shouldShowRequestPermissionRationale(activity, Manifest.permission.POST_NOTIFICATIONS)))
                if (canAsk) {
                    asked = true
                    permissionLauncher.launch(Manifest.permission.POST_NOTIFICATIONS)
                } else {
                    // Denied for good (or switched off in settings): only the system screen can fix it.
                    context.startActivity(
                        Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS)
                            .putExtra(Settings.EXTRA_APP_PACKAGE, context.packageName)
                            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
                    )
                }
            }
        }
        if (!exactOk && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            BannerCard(
                title = stringResource(R.string.perm_exact_title),
                body = stringResource(R.string.perm_exact_body),
                action = stringResource(R.string.perm_open_settings),
            ) {
                context.startActivity(
                    Intent(Settings.ACTION_REQUEST_SCHEDULE_EXACT_ALARM, Uri.parse("package:${context.packageName}"))
                        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
                )
            }
        }
    }
}

@Composable
private fun BannerCard(title: String, body: String, action: String, onAction: () -> Unit) {
    Surface(shape = RoundedCornerShape(16.dp), color = MaterialTheme.colorScheme.errorContainer) {
        Column(Modifier.fillMaxWidth().padding(14.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(title, style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.onErrorContainer)
            Text(body, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onErrorContainer)
            TextButton(onClick = onAction) { Text(action) }
        }
    }
}
