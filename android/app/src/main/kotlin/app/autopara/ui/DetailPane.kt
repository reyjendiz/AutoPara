package app.autopara.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import app.autopara.R
import app.autopara.core.link.MeetingLink
import app.autopara.core.model.Occurrence
import app.autopara.core.model.OccurrenceStatus
import app.autopara.core.model.Provider

/** Everything about one class on one date, with the two things you can do: open it, skip it. */
@Composable
fun LessonDetail(
    occurrence: Occurrence,
    status: OccurrenceStatus?,
    onOpen: () -> Unit,
    onToggleSkip: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val lesson = occurrence.lesson
    val locale = currentLocale()
    val linked = MeetingLink.isOpenable(lesson.url)
    val provider = lesson.provider.takeIf { it != Provider.UNKNOWN } ?: MeetingLink.providerOf(lesson.url)

    Column(
        modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(horizontal = 20.dp, vertical = 16.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Text(lesson.subject, style = MaterialTheme.typography.headlineSmall)
        Text(
            "${dayTitle(occurrence.date, locale)} · ${lesson.timeRange()}",
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        if (lesson.teacher.isNotBlank()) {
            Text(lesson.teacher.trim('(', ')'), style = MaterialTheme.typography.bodyLarge)
        }
        if (lesson.groupNames.size > 1) {
            Text(
                stringResource(R.string.detail_groups, lesson.groupNames.joinToString(", ")),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            ProviderChip(lesson)
            StatusText(status)
        }
        Spacer(Modifier.height(4.dp))
        if (linked) {
            Button(onClick = onOpen, modifier = Modifier.fillMaxWidth()) {
                Text(
                    stringResource(
                        when (provider) {
                            Provider.ZOOM -> R.string.detail_open_zoom
                            Provider.GOOGLE_MEET -> R.string.detail_open_meet
                            Provider.UNKNOWN -> R.string.detail_open_link
                        },
                    ),
                )
            }
        }
        OutlinedButton(onClick = onToggleSkip, modifier = Modifier.fillMaxWidth()) {
            Text(stringResource(if (status == OccurrenceStatus.SKIPPED) R.string.detail_unskip else R.string.detail_skip))
        }
    }
}
