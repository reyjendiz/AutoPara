package app.autopara.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import app.autopara.R
import app.autopara.core.model.Occurrence
import app.autopara.core.schedule.CalendarMath
import java.time.LocalDate

/** One day as a list of cards -- the view phones live in. */
@Composable
fun DayView(
    state: UiState,
    selected: Occurrence?,
    onSelect: (Occurrence) -> Unit,
    onEdit: (Occurrence) -> Unit,
    modifier: Modifier = Modifier,
) {
    val occurrences = CalendarMath.occurrencesOn(state.visibleLessons, state.date)
    val today = LocalDate.now()
    val nextUp = state.nextUp?.takeIf { state.date == today }

    LazyColumn(
        modifier = modifier.fillMaxSize(),
        contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 96.dp), // room for the + button
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        if (occurrences.isEmpty()) {
            item { EmptyDay(filtered = state.filter.isActive) }
        }
        items(occurrences, key = { it.lesson.id }) { occurrence ->
            val ongoing = nextUp?.ongoing == true && nextUp.lesson.id == occurrence.lesson.id
            LessonCard(
                lesson = occurrence.lesson,
                status = state.statuses[occurrence.lesson.id to occurrence.date],
                ongoing = ongoing,
                selected = selected?.lesson?.id == occurrence.lesson.id && selected.date == occurrence.date,
                onClick = { onSelect(occurrence) },
                onEdit = { onEdit(occurrence) },
                modifier = Modifier.fillMaxWidth().widthIn(max = 720.dp),
            )
        }
    }
}

@Composable
private fun EmptyDay(filtered: Boolean) {
    Column(
        Modifier.fillMaxWidth().padding(vertical = 48.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Text(
            stringResource(if (filtered) R.string.no_matches else R.string.no_classes),
            style = MaterialTheme.typography.titleMedium,
        )
        if (!filtered) {
            Text(
                stringResource(R.string.no_classes_hint),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

/** "Now · Psychology · 40 min left": tap it to open the class. */
@Composable
fun NextUpPill(state: UiState, onClick: (Occurrence) -> Unit, modifier: Modifier = Modifier) {
    val item = state.nextUp ?: return
    Surface(
        shape = RoundedCornerShape(50),
        color = MaterialTheme.colorScheme.primaryContainer,
        modifier = modifier.clickable { onClick(Occurrence(item.lesson, state.now.toLocalDate())) },
    ) {
        Row(Modifier.padding(horizontal = 14.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(
                nextUpText(item),
                style = MaterialTheme.typography.labelLarge,
                color = MaterialTheme.colorScheme.onPrimaryContainer,
                maxLines = 1,
                overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis,
            )
        }
    }
}
