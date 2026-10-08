package app.autopara.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.autopara.R
import app.autopara.core.link.MeetingLink
import app.autopara.core.model.Lesson
import app.autopara.core.model.OccurrenceStatus
import app.autopara.core.model.Provider

@Composable
fun isDarkSurface(): Boolean = MaterialTheme.colorScheme.background.luminance() < 0.5f

/** A card's fill: the surface with a faint wash of its subject colour. */
@Composable
fun tintedFill(subject: String): Color =
    lerp(MaterialTheme.colorScheme.surface, subjectColor(subject), tintAmount(isDarkSurface()))

/** The text colour a status/provider chip uses; pastel on dark, saturated on light. */
@Composable
private fun chipInk(light: Long, dark: Long): Color = Color(if (isDarkSurface()) dark else light)

@Composable
fun ProviderChip(lesson: Lesson, modifier: Modifier = Modifier) {
    val provider = lesson.provider.takeIf { it != Provider.UNKNOWN } ?: MeetingLink.providerOf(lesson.url)
    val linked = MeetingLink.isOpenable(lesson.url)
    val (label, ink) = when {
        !linked -> stringResource(R.string.no_link) to chipInk(0xFF7D6200, 0xFFF1F36A)
        provider == Provider.ZOOM -> stringResource(R.string.provider_zoom) to chipInk(0xFF1F67AD, 0xFF86CDF7)
        provider == Provider.GOOGLE_MEET -> stringResource(R.string.provider_meet) to chipInk(0xFF1F7A47, 0xFF8FE8A8)
        else -> stringResource(R.string.provider_link) to MaterialTheme.colorScheme.onSurfaceVariant
    }
    Text(
        text = label,
        color = ink,
        fontSize = 11.sp,
        maxLines = 1,
        modifier = modifier
            .background(ink.copy(alpha = 0.14f), RoundedCornerShape(50))
            .padding(horizontal = 8.dp, vertical = 2.dp),
    )
}

@Composable
fun StatusText(status: OccurrenceStatus?) {
    val label = when (status) {
        OccurrenceStatus.OPENED -> stringResource(R.string.status_opened)
        OccurrenceStatus.SKIPPED -> stringResource(R.string.status_skipped)
        else -> return
    }
    Text(label, fontSize = 11.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
}

/** A class in the day list: subject colour bar, times, subject, teacher, link chip, status. */
@Composable
fun LessonCard(
    lesson: Lesson,
    status: OccurrenceStatus?,
    ongoing: Boolean,
    selected: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val skipped = status == OccurrenceStatus.SKIPPED
    Surface(
        onClick = onClick,
        shape = RoundedCornerShape(16.dp),
        color = tintedFill(lesson.subject),
        tonalElevation = if (selected || ongoing) 3.dp else 0.dp,
        border = if (selected) androidx.compose.foundation.BorderStroke(1.5.dp, MaterialTheme.colorScheme.primary) else null,
        modifier = modifier.alpha(if (skipped) 0.55f else 1f),
    ) {
        Row(Modifier.height(IntrinsicSize.Min)) {
            Box(
                Modifier
                    .width(5.dp)
                    .fillMaxHeight()
                    .background(subjectColor(lesson.subject)),
            )
            Column(Modifier.padding(horizontal = 14.dp, vertical = 12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(lesson.timeRange(), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    StatusText(status)
                }
                Text(
                    lesson.subject,
                    style = MaterialTheme.typography.titleMedium,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 3,
                    overflow = TextOverflow.Ellipsis,
                )
                if (lesson.teacher.isNotBlank()) {
                    Text(
                        lesson.teacher.trim('(', ')'),
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                Spacer(Modifier.height(2.dp))
                ProviderChip(lesson)
            }
        }
    }
}
