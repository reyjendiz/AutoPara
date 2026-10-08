package app.autopara.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import app.autopara.BuildConfig
import app.autopara.R
import app.autopara.data.ThemeMode

@Composable
fun SettingsScreen(
    state: UiState,
    onTheme: (ThemeMode) -> Unit,
    onReminders: (Boolean) -> Unit,
    onImport: () -> Unit,
    banner: @Composable () -> Unit,
    modifier: Modifier = Modifier,
) {
    Box(modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
        Column(
            Modifier
                .widthIn(max = 640.dp)
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Text(stringResource(R.string.settings_title), style = MaterialTheme.typography.headlineSmall)

            Section(stringResource(R.string.settings_theme)) {
                val modes = ThemeMode.entries
                SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
                    modes.forEachIndexed { index, mode ->
                        SegmentedButton(
                            selected = state.settings.theme == mode,
                            onClick = { onTheme(mode) },
                            shape = SegmentedButtonDefaults.itemShape(index, modes.size),
                        ) {
                            Text(
                                stringResource(
                                    when (mode) {
                                        ThemeMode.SYSTEM -> R.string.theme_system
                                        ThemeMode.LIGHT -> R.string.theme_light
                                        ThemeMode.DARK -> R.string.theme_dark
                                    },
                                ),
                            )
                        }
                    }
                }
            }

            Section(stringResource(R.string.settings_reminders)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        stringResource(R.string.settings_reminders_desc),
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.weight(1f),
                    )
                    Switch(checked = state.settings.remindersEnabled, onCheckedChange = onReminders)
                }
                if (state.settings.remindersEnabled) banner()
            }

            Section(stringResource(R.string.settings_import)) {
                Text(
                    stringResource(R.string.settings_import_desc),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                if (state.importing) {
                    CircularProgressIndicator()
                } else {
                    OutlinedButton(onClick = onImport) { Text(stringResource(R.string.import_button)) }
                }
            }

            Text(
                stringResource(R.string.settings_about, BuildConfig.VERSION_NAME),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun Section(title: String, content: @Composable () -> Unit) {
    Surface(shape = RoundedCornerShape(20.dp), color = MaterialTheme.colorScheme.surface) {
        Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Text(title, style = MaterialTheme.typography.titleMedium)
            content()
        }
    }
}
