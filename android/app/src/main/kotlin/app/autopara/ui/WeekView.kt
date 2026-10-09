package app.autopara.ui

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.ui.draw.clip
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.autopara.core.importer.Normalize
import app.autopara.core.model.Lesson
import app.autopara.core.model.Occurrence
import app.autopara.core.model.OccurrenceStatus
import app.autopara.core.schedule.CalendarMath
import java.time.LocalDate
import java.time.LocalTime

private val TimeColumnWidth = 48.dp

/** Below this a day column is too narrow to read, so the grid scrolls sideways instead of squeezing. */
private val MinDayColumnWidth = 92.dp
private val RowHeight = 84.dp
private const val FIRST_PAIRS = 6

/** How many grid rows a class covers: a merged double class spans several pairs. */
private fun Lesson.span(): Int =
    maxOf(1, Normalize.lastPairCovered(start.toString(), end.toString()) - pair + 1)

/**
 * The week as seven day columns and one row per pair, like the desktop grid. Cards are placed by
 * their pair and stretched over the pairs they cover; classes sharing a slot split its width.
 */
@Composable
fun WeekView(
    state: UiState,
    selected: Occurrence?,
    onSelect: (Occurrence) -> Unit,
    onEdit: (Occurrence) -> Unit,
    onDayClick: (LocalDate) -> Unit,
    modifier: Modifier = Modifier,
) {
    val locale = currentLocale()
    val days = CalendarMath.weekDays(state.date)
    val rows = maxOf(FIRST_PAIRS, state.visibleLessons.maxOfOrNull { it.pair + it.span() - 1 } ?: 0)
    val today = LocalDate.now()
    val lineColor = MaterialTheme.colorScheme.outlineVariant

    BoxWithConstraints(modifier.fillMaxSize()) {
    // Seven readable columns need more than a phone in portrait has: scroll the whole grid sideways.
    val needed = TimeColumnWidth + MinDayColumnWidth * 7 + 8.dp
    val scrolls = maxWidth < needed
    val gridWidth = if (scrolls) needed else maxWidth
    Box(Modifier.fillMaxSize().then(if (scrolls) Modifier.horizontalScroll(rememberScrollState()) else Modifier)) {
    Column(Modifier.width(gridWidth).fillMaxHeight()) {
        // Day headers.
        Row(Modifier.fillMaxWidth().padding(end = 8.dp)) {
            Spacer(Modifier.width(TimeColumnWidth))
            for (day in days) {
                val isToday = day == today
                Column(
                    Modifier.weight(1f).clickable { onDayClick(day) }.padding(vertical = 6.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Text(
                        shortWeekday(day.dayOfWeek, locale),
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    Box(
                        Modifier
                            .background(if (isToday) MaterialTheme.colorScheme.primary else androidx.compose.ui.graphics.Color.Transparent, CircleShape)
                            .padding(horizontal = 8.dp, vertical = 3.dp),
                    ) {
                        Text(
                            dayNumber(day),
                            style = MaterialTheme.typography.labelLarge,
                            color = if (isToday) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurface,
                        )
                    }
                }
            }
        }

        Row(
            Modifier
                .weight(1f)
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(end = 8.dp, bottom = 96.dp), // room for the + button
        ) {
            // Time labels: the pair number and its start time.
            Column(Modifier.width(TimeColumnWidth)) {
                for (pair in 1..rows) {
                    Column(Modifier.height(RowHeight).padding(end = 6.dp), horizontalAlignment = Alignment.End) {
                        Text(pair.toString(), fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurface)
                        Text(
                            Normalize.pairStartTime(pair),
                            fontSize = 10.sp,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }
            for (day in days) {
                DayColumn(
                    day = day,
                    rows = rows,
                    state = state,
                    selected = selected,
                    lineColor = lineColor,
                    isToday = day == today,
                    onSelect = onSelect,
                    onEdit = onEdit,
                    modifier = Modifier.weight(1f),
                )
            }
        }
    }
    }
    }
}

@Composable
private fun DayColumn(
    day: LocalDate,
    rows: Int,
    state: UiState,
    selected: Occurrence?,
    lineColor: androidx.compose.ui.graphics.Color,
    isToday: Boolean,
    onSelect: (Occurrence) -> Unit,
    onEdit: (Occurrence) -> Unit,
    modifier: Modifier = Modifier,
) {
    val occurrences = CalendarMath.occurrencesOn(state.visibleLessons, day)
    val nowColor = MaterialTheme.colorScheme.error
    Box(
        modifier
            .height(RowHeight * rows)
            .drawBehind {
                // Row separators and the column's left edge.
                for (row in 0..rows) {
                    val y = RowHeight.toPx() * row
                    drawLine(lineColor, Offset(0f, y), Offset(size.width, y), strokeWidth = 1f)
                }
                drawLine(lineColor, Offset(0f, 0f), Offset(0f, size.height), strokeWidth = 1f)
            },
    ) {
        for ((pair, group) in occurrences.groupBy { it.lesson.pair }) {
            val span = group.maxOf { it.lesson.span() }
            Row(
                Modifier
                    .offset(y = RowHeight * (pair - 1))
                    .height(RowHeight * span)
                    .fillMaxWidth()
                    .padding(2.dp),
                horizontalArrangement = Arrangement.spacedBy(2.dp),
            ) {
                for (occurrence in group) {
                    WeekCell(
                        occurrence = occurrence,
                        status = state.statuses[occurrence.lesson.id to occurrence.date],
                        selected = selected?.lesson?.id == occurrence.lesson.id && selected.date == occurrence.date,
                        onClick = { onSelect(occurrence) },
                        onLongClick = { onEdit(occurrence) },
                        modifier = Modifier.weight(1f).fillMaxHeight(),
                    )
                }
            }
        }

        // The current-time line, in today's column only.
        if (isToday) {
            nowOffset(state.now.toLocalTime(), rows)?.let { y ->
                Box(
                    Modifier
                        .offset(y = y)
                        .fillMaxWidth()
                        .height(2.dp)
                        .alpha(0.5f)
                        .background(nowColor),
                )
            }
        }
    }
}

/** Where the clock falls in the grid: linear inside a pair's row, none outside the grid. */
private fun nowOffset(now: LocalTime, rows: Int): Dp? {
    val minutes = now.hour * 60 + now.minute
    for (pair in 1..rows) {
        val start = minutesOf(Normalize.pairStartTime(pair))
        val next = if (pair < rows) minutesOf(Normalize.pairStartTime(pair + 1)) else start + 90
        if (minutes in start until next) {
            val fraction = (minutes - start).toFloat() / (next - start)
            return RowHeight * (pair - 1) + RowHeight * fraction
        }
    }
    return null
}

private fun minutesOf(hhmm: String): Int {
    val (h, m) = hhmm.split(":").map { it.toInt() }
    return h * 60 + m
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun WeekCell(
    occurrence: Occurrence,
    status: OccurrenceStatus?,
    selected: Boolean,
    onClick: () -> Unit,
    onLongClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val lesson = occurrence.lesson
    val shape = RoundedCornerShape(8.dp)
    Surface(
        shape = shape,
        color = tintedFill(lesson.subject),
        border = if (selected) androidx.compose.foundation.BorderStroke(1.5.dp, MaterialTheme.colorScheme.primary) else null,
        modifier = modifier
            .alpha(if (status == OccurrenceStatus.SKIPPED) 0.5f else 1f)
            .clip(shape)
            .combinedClickable(onClick = onClick, onLongClick = onLongClick),
    ) {
        Row {
            Box(Modifier.width(3.dp).fillMaxHeight().background(subjectColor(lesson.subject)))
            Column(Modifier.padding(horizontal = 4.dp, vertical = 3.dp)) {
                Text(
                    lesson.start.clock(),
                    fontSize = 9.sp,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                )
                Text(
                    lesson.subject,
                    fontSize = 11.sp,
                    lineHeight = 13.sp,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 4,
                    overflow = TextOverflow.Ellipsis,
                    textAlign = TextAlign.Start,
                )
            }
        }
    }
}
