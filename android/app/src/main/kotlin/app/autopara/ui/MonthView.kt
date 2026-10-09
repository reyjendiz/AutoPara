package app.autopara.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.ui.draw.clip
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.autopara.core.schedule.CalendarMath
import java.time.DayOfWeek
import java.time.LocalDate
import java.time.YearMonth

private const val MAX_CHIPS = 3
private const val MAX_DOTS = 5

/**
 * The month as a grid of days. Wide screens name the classes in each cell; narrow ones show a dot
 * per class in its subject colour, which stays legible at phone width.
 */
@Composable
fun MonthView(
    state: UiState,
    onDayClick: (LocalDate) -> Unit,
    onDayLongClick: (LocalDate) -> Unit,
    modifier: Modifier = Modifier,
) {
    val locale = currentLocale()
    val month = YearMonth.from(state.date)
    val weeks = CalendarMath.monthGrid(month)
    val today = LocalDate.now()

    BoxWithConstraints(modifier.fillMaxSize().padding(horizontal = 12.dp)) {
        val wide = maxWidth >= 600.dp
        Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Row(Modifier.fillMaxWidth()) {
                for (dow in DayOfWeek.entries) {
                    Text(
                        shortWeekday(dow, locale),
                        modifier = Modifier.weight(1f),
                        textAlign = TextAlign.Center,
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            for (week in weeks) {
                Row(Modifier.weight(1f).fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    for (day in week) {
                        MonthCell(
                            day = day,
                            inMonth = YearMonth.from(day) == month,
                            isToday = day == today,
                            isSelected = day == state.date,
                            lessons = CalendarMath.occurrencesOn(state.visibleLessons, day).map { it.lesson },
                            wide = wide,
                            onClick = { onDayClick(day) },
                            onLongClick = { onDayLongClick(day) },
                            modifier = Modifier.weight(1f).fillMaxSize(),
                        )
                    }
                }
            }
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun MonthCell(
    day: LocalDate,
    inMonth: Boolean,
    isToday: Boolean,
    isSelected: Boolean,
    lessons: List<app.autopara.core.model.Lesson>,
    wide: Boolean,
    onClick: () -> Unit,
    onLongClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val shape = RoundedCornerShape(10.dp)
    Surface(
        shape = shape,
        color = MaterialTheme.colorScheme.surface,
        border = if (isSelected) BorderStroke(1.5.dp, MaterialTheme.colorScheme.primary) else null,
        modifier = modifier
            .alpha(if (inMonth) 1f else 0.45f)
            .clip(shape)
            .combinedClickable(onClick = onClick, onLongClick = onLongClick),
    ) {
        Column(Modifier.padding(4.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(
                dayNumber(day),
                modifier = Modifier
                    .background(if (isToday) MaterialTheme.colorScheme.primary else androidx.compose.ui.graphics.Color.Transparent, CircleShape)
                    .padding(horizontal = 6.dp, vertical = 1.dp),
                style = MaterialTheme.typography.labelMedium,
                color = if (isToday) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurface,
            )
            if (wide) {
                for (lesson in lessons.take(MAX_CHIPS)) {
                    Text(
                        lesson.subject,
                        fontSize = 10.sp,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        color = MaterialTheme.colorScheme.onSurface,
                        modifier = Modifier
                            .fillMaxWidth()
                            .background(tintedFill(lesson.subject), RoundedCornerShape(4.dp))
                            .padding(horizontal = 4.dp, vertical = 1.dp),
                    )
                }
                if (lessons.size > MAX_CHIPS) {
                    Text("+${lessons.size - MAX_CHIPS}", fontSize = 10.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            } else {
                Row(horizontalArrangement = Arrangement.spacedBy(2.dp), verticalAlignment = Alignment.CenterVertically) {
                    for (lesson in lessons.take(MAX_DOTS)) {
                        androidx.compose.foundation.layout.Box(Modifier.size(6.dp).background(subjectColor(lesson.subject), CircleShape))
                    }
                }
            }
        }
    }
}
