package app.autopara.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.background
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowLeft
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.autopara.R
import app.autopara.core.model.Occurrence
import app.autopara.core.schedule.CalendarMath
import app.autopara.data.ViewMode
import java.time.LocalDate
import kotlin.math.abs

/** What the schedule screen can ask of its host. */
data class ScheduleActions(
    val onShift: (direction: Int) -> Unit,
    val onToday: () -> Unit,
    val onSelectDate: (LocalDate) -> Unit,
    val onViewMode: (ViewMode) -> Unit,
    val onSelect: (Occurrence) -> Unit,
    val onSearch: (String) -> Unit,
    val onSubject: (String) -> Unit,
    val onClearFilter: () -> Unit,
    val onJumpToMatch: (direction: Int) -> Unit,
    val onGroup: (Long) -> Unit,
    val onEdit: (Occurrence) -> Unit,
    val onAddOnDay: (LocalDate) -> Unit,
)

/** Day, week and month are offered on every screen; the week grid scrolls sideways when it is narrow. */
fun availableModes(): List<ViewMode> = listOf(ViewMode.DAY, ViewMode.WEEK, ViewMode.MONTH)

/** The stored choice, or the right default for this screen: a day on a phone, a week on a wider screen. */
fun effectiveMode(stored: ViewMode?, compactWidth: Boolean): ViewMode =
    stored ?: if (compactWidth) ViewMode.DAY else ViewMode.WEEK

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ScheduleScreen(
    state: UiState,
    compactWidth: Boolean,
    actions: ScheduleActions,
    modifier: Modifier = Modifier,
    banner: @Composable () -> Unit = {},
) {
    val locale = currentLocale()
    val mode = effectiveMode(state.settings.viewMode, compactWidth)
    var searchOpen by rememberSaveable { mutableStateOf(false) }
    val showSearch = searchOpen || state.filter.isActive

    Column(modifier.fillMaxSize()) {
        // ---- header: period title + group, navigation, search
        Row(
            Modifier.fillMaxWidth().padding(start = 16.dp, end = 4.dp, top = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f)) {
                Text(
                    periodTitle(mode, state.date, CalendarMath.weekStart(state.date), locale),
                    style = MaterialTheme.typography.titleLarge,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                GroupPicker(state, actions.onGroup)
            }
            IconButton(onClick = { searchOpen = !searchOpen }) {
                Icon(Icons.Filled.Search, contentDescription = stringResource(R.string.search))
            }
            IconButton(onClick = { actions.onShift(-1) }) {
                Icon(Icons.AutoMirrored.Filled.KeyboardArrowLeft, contentDescription = stringResource(R.string.nav_previous))
            }
            TextButton(onClick = actions.onToday) { Text(stringResource(R.string.today)) }
            IconButton(onClick = { actions.onShift(1) }) {
                Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, contentDescription = stringResource(R.string.nav_next))
            }
        }

        // ---- the next class, as a pill (wider screens put it in the side pane as well)
        if (state.nextUp != null) {
            NextUpPill(
                state = state,
                onClick = actions.onSelect,
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 6.dp),
            )
        }

        banner()

        AnimatedVisibility(showSearch) {
            FilterBar(state, actions) {
                searchOpen = false
                actions.onClearFilter()
            }
        }

        // ---- view switcher
        SingleChoiceSegmentedButtonRow(Modifier.padding(horizontal = 16.dp, vertical = 8.dp).fillMaxWidth()) {
            val modes = availableModes()
            modes.forEachIndexed { index, option ->
                SegmentedButton(
                    selected = option == mode,
                    onClick = { actions.onViewMode(option) },
                    shape = SegmentedButtonDefaults.itemShape(index, modes.size),
                ) {
                    Text(
                        stringResource(
                            when (option) {
                                ViewMode.DAY -> R.string.view_day
                                ViewMode.WEEK -> R.string.view_week
                                ViewMode.MONTH -> R.string.view_month
                            },
                        ),
                    )
                }
            }
        }

        if (mode == ViewMode.DAY && compactWidth) {
            WeekStrip(state, actions.onSelectDate)
        }

        // ---- the calendar itself; a horizontal swipe moves one step
        val selected = state.selected
        Box(
            Modifier
                .weight(1f)
                .fillMaxWidth()
                .swipeToShift(mode, actions.onShift),
        ) {
            when (mode) {
                ViewMode.DAY -> DayView(state, selected, actions.onSelect, actions.onEdit)
                ViewMode.WEEK -> WeekView(
                    state, selected, actions.onSelect, actions.onEdit,
                    onDayClick = { day ->
                        actions.onSelectDate(day)
                        actions.onViewMode(ViewMode.DAY)
                    },
                )
                ViewMode.MONTH -> MonthView(
                    state,
                    onDayClick = { day ->
                        actions.onSelectDate(day)
                        actions.onViewMode(ViewMode.DAY)
                    },
                    onDayLongClick = actions.onAddOnDay,
                )
            }
        }
    }
}

/** A horizontal swipe past a threshold moves the calendar one step; vertical scrolling is untouched. */
private fun Modifier.swipeToShift(mode: ViewMode, onShift: (Int) -> Unit): Modifier =
    pointerInput(mode) {
        var total = 0f
        detectHorizontalDragGestures(
            onDragStart = { total = 0f },
            onDragEnd = { if (abs(total) > SWIPE_THRESHOLD_PX) onShift(if (total < 0) 1 else -1) },
            onDragCancel = { total = 0f },
            onHorizontalDrag = { _, delta -> total += delta },
        )
    }

private const val SWIPE_THRESHOLD_PX = 160f

/** The group (and course) being shown; tapping it lists the others. */
@Composable
private fun GroupPicker(state: UiState, onGroup: (Long) -> Unit) {
    val group = state.selectedGroup ?: return
    var open by rememberSaveable { mutableStateOf(false) }
    val courseName = { courseId: Long -> state.courses.firstOrNull { it.id == courseId }?.name.orEmpty() }
    Box {
        Row(
            Modifier.clickable(enabled = state.groups.size > 1) { open = true },
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                stringResource(R.string.course_group, courseName(group.courseId), group.name),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (state.groups.size > 1) {
                Icon(Icons.Filled.KeyboardArrowDown, contentDescription = stringResource(R.string.group_picker), modifier = Modifier.size(18.dp))
            }
        }
        DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            for (option in state.groups) {
                DropdownMenuItem(
                    text = { Text(stringResource(R.string.course_group, courseName(option.courseId), option.name)) },
                    onClick = {
                        open = false
                        onGroup(option.id)
                    },
                )
            }
        }
    }
}

/** Mon..Sun of the shown week as tappable days, with a dot where a class falls. */
@Composable
private fun WeekStrip(state: UiState, onSelectDate: (LocalDate) -> Unit) {
    val locale = currentLocale()
    val today = LocalDate.now()
    Row(Modifier.fillMaxWidth().padding(horizontal = 8.dp)) {
        for (day in CalendarMath.weekDays(state.date)) {
            val isSelected = day == state.date
            val hasClasses = state.visibleLessons.any { it.occursOn(day) }
            Column(
                Modifier.weight(1f).clickable { onSelectDate(day) }.padding(vertical = 4.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Text(
                    shortWeekday(day.dayOfWeek, locale),
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Box(
                    Modifier
                        .padding(vertical = 2.dp)
                        .background(if (isSelected) MaterialTheme.colorScheme.primary else androidx.compose.ui.graphics.Color.Transparent, CircleShape)
                        .padding(horizontal = 9.dp, vertical = 5.dp),
                ) {
                    Text(
                        dayNumber(day),
                        style = MaterialTheme.typography.labelLarge,
                        color = when {
                            isSelected -> MaterialTheme.colorScheme.onPrimary
                            day == today -> MaterialTheme.colorScheme.tertiary
                            else -> MaterialTheme.colorScheme.onSurface
                        },
                    )
                }
                Box(
                    Modifier
                        .size(5.dp)
                        .background(
                            if (hasClasses) MaterialTheme.colorScheme.outline else androidx.compose.ui.graphics.Color.Transparent,
                            CircleShape,
                        ),
                )
            }
        }
    }
}

/** Text search plus a subject chip, with arrows to jump to the previous / next matching day. */
@Composable
private fun FilterBar(state: UiState, actions: ScheduleActions, onClose: () -> Unit) {
    var subjectMenu by rememberSaveable { mutableStateOf(false) }
    Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 4.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        OutlinedTextField(
            value = state.filter.text,
            onValueChange = actions.onSearch,
            modifier = Modifier.fillMaxWidth(),
            singleLine = true,
            placeholder = { Text(stringResource(R.string.search_hint)) },
            leadingIcon = { Icon(Icons.Filled.Search, contentDescription = null) },
            trailingIcon = {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    if (state.filter.isActive) {
                        IconButton(onClick = { actions.onJumpToMatch(-1) }) {
                            Icon(Icons.AutoMirrored.Filled.KeyboardArrowLeft, contentDescription = stringResource(R.string.match_previous))
                        }
                        IconButton(onClick = { actions.onJumpToMatch(1) }) {
                            Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, contentDescription = stringResource(R.string.match_next))
                        }
                    }
                    IconButton(onClick = onClose) {
                        Icon(Icons.Filled.Close, contentDescription = stringResource(R.string.close))
                    }
                }
            },
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
        )
        if (state.subjects.isNotEmpty()) {
            Box {
                FilterChip(
                    selected = state.filter.subject.isNotEmpty(),
                    onClick = { subjectMenu = true },
                    label = {
                        Text(
                            state.filter.subject.ifEmpty { stringResource(R.string.filter_all_subjects) },
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    },
                    trailingIcon = { Icon(Icons.Filled.KeyboardArrowDown, contentDescription = null, modifier = Modifier.size(18.dp)) },
                )
                DropdownMenu(expanded = subjectMenu, onDismissRequest = { subjectMenu = false }) {
                    DropdownMenuItem(
                        text = { Text(stringResource(R.string.filter_all_subjects)) },
                        onClick = {
                            subjectMenu = false
                            actions.onSubject("")
                        },
                    )
                    for (subject in state.subjects) {
                        DropdownMenuItem(
                            text = { Text(subject) },
                            onClick = {
                                subjectMenu = false
                                actions.onSubject(subject)
                            },
                        )
                    }
                }
            }
        }
    }
}
