package app.autopara.notify

import android.app.AlarmManager
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import app.autopara.AutoParaApp
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeout

/**
 * Re-arms the alarms after anything that silently drops or invalidates them: a reboot, an app
 * update, a clock or time-zone change, or the user granting exact-alarm access.
 */
class RescheduleReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action !in HANDLED) return
        val pending = goAsync()
        val container = (context.applicationContext as AutoParaApp).container
        container.appScope.launch {
            try {
                withTimeout(8_000L) { container.reminders.rearm() }
            } finally {
                pending.finish()
            }
        }
    }

    private companion object {
        val HANDLED = setOf(
            Intent.ACTION_BOOT_COMPLETED,
            Intent.ACTION_MY_PACKAGE_REPLACED,
            Intent.ACTION_TIME_CHANGED,
            Intent.ACTION_TIMEZONE_CHANGED,
            AlarmManager.ACTION_SCHEDULE_EXACT_ALARM_PERMISSION_STATE_CHANGED,
        )
    }
}
