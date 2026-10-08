package app.autopara.ui

import android.content.Context
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.DateRange
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationRail
import androidx.compose.material3.NavigationRailItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.SnackbarResult
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.windowsizeclass.WindowSizeClass
import androidx.compose.material3.windowsizeclass.WindowWidthSizeClass
import androidx.compose.material3.windowsizeclass.WindowHeightSizeClass
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.autopara.R
import app.autopara.core.model.Occurrence
import app.autopara.core.model.Provider
import app.autopara.link.LaunchResult
import app.autopara.link.MeetingLauncher

private enum class Tab { SCHEDULE, SETTINGS }

/**
 * The whole UI. The window size class decides the shape:
 *
 * * **compact** (phone, portrait): bottom navigation, one column; a class opens in a bottom sheet.
 * * **medium** (small tablet, phone in landscape): a navigation rail, the week grid becomes available.
 * * **expanded** (tablet): rail + calendar + a permanent detail pane beside it.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AutoParaRoot(viewModel: MainViewModel, windowSizeClass: WindowSizeClass) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val context = LocalContext.current

    val compactWidth = windowSizeClass.widthSizeClass == WindowWidthSizeClass.Compact
    val expanded = windowSizeClass.widthSizeClass == WindowWidthSizeClass.Expanded
    // A phone held sideways is wide but short: a rail uses its width better than a bottom bar.
    val useRail = !compactWidth || windowSizeClass.heightSizeClass == WindowHeightSizeClass.Compact

    val snackbar = remember { SnackbarHostState() }
    var tab by rememberSaveable { mutableStateOf(Tab.SCHEDULE) }

    val picker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) viewModel.importFrom(uri)
    }
    val pickDocument = { picker.launch(arrayOf("*/*")) }

    val openLink: (Occurrence) -> Unit = { occurrence ->
        when (val result = MeetingLauncher.open(context, occurrence.lesson.url)) {
            LaunchResult.OpenedInApp -> viewModel.onLinkResult(occurrence, true)
            is LaunchResult.OpenedInBrowser -> {
                viewModel.onLinkResult(occurrence, true)
                result.missingApp?.let { viewModel.showMessage(UiMessage.AppMissing(it)) }
            }
            is LaunchResult.OpenedStore -> viewModel.onLinkResult(occurrence, false)
            LaunchResult.NotALink -> viewModel.showMessage(UiMessage.NoLink)
            LaunchResult.Failed -> viewModel.showMessage(UiMessage.LinkFailed)
        }
    }

    // One-shot messages become snackbars.
    LaunchedEffect(state.message) {
        val message = state.message ?: return@LaunchedEffect
        val text = messageText(context, message)
        if (message is UiMessage.AppMissing) {
            val result = snackbar.showSnackbar(text, actionLabel = context.getString(R.string.app_missing_get))
            if (result == SnackbarResult.ActionPerformed) MeetingLauncher.openStore(context, message.provider)
        } else {
            snackbar.showSnackbar(text)
        }
        viewModel.consumeMessage()
    }

    AutoParaTheme(state.settings.theme) {
        Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            when {
                !state.loaded -> Box(Modifier.fillMaxSize())
                !state.hasTimetable -> Scaffold(
                    snackbarHost = { SnackbarHost(snackbar) },
                    containerColor = MaterialTheme.colorScheme.background,
                ) { padding ->
                    ImportScreen(state.importing, onPick = pickDocument, modifier = Modifier.padding(padding))
                }
                else -> {
                    val shift = { direction: Int ->
                        viewModel.shift(direction, effectiveMode(state.settings.viewMode, compactWidth))
                    }
                    val actions = ScheduleActions(
                        onShift = shift,
                        onToday = viewModel::goToday,
                        onSelectDate = viewModel::selectDate,
                        onViewMode = { viewModel.setViewMode(it) },
                        onSelect = { viewModel.select(it.lesson.id, it.date) },
                        onSearch = viewModel::setSearch,
                        onSubject = viewModel::setSubject,
                        onClearFilter = viewModel::clearFilter,
                        onJumpToMatch = viewModel::jumpToMatch,
                        onGroup = viewModel::selectGroup,
                    )
                    val reminderBanner: @Composable () -> Unit = {
                        ReminderBanner(
                            enabled = state.settings.remindersEnabled,
                            modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
                        )
                    }

                    Scaffold(
                        snackbarHost = { SnackbarHost(snackbar) },
                        containerColor = MaterialTheme.colorScheme.background,
                        contentWindowInsets = WindowInsets.safeDrawing,
                        bottomBar = {
                            if (!useRail) {
                                NavigationBar {
                                    NavigationBarItem(
                                        selected = tab == Tab.SCHEDULE,
                                        onClick = { tab = Tab.SCHEDULE },
                                        icon = { Icon(Icons.Filled.DateRange, contentDescription = null) },
                                        label = { Text(stringResource(R.string.nav_schedule)) },
                                    )
                                    NavigationBarItem(
                                        selected = tab == Tab.SETTINGS,
                                        onClick = { tab = Tab.SETTINGS },
                                        icon = { Icon(Icons.Filled.Settings, contentDescription = null) },
                                        label = { Text(stringResource(R.string.nav_settings)) },
                                    )
                                }
                            }
                        },
                    ) { padding ->
                        Row(Modifier.padding(padding).fillMaxSize()) {
                            if (useRail) {
                                NavigationRail {
                                    NavigationRailItem(
                                        selected = tab == Tab.SCHEDULE,
                                        onClick = { tab = Tab.SCHEDULE },
                                        icon = { Icon(Icons.Filled.DateRange, contentDescription = null) },
                                        label = { Text(stringResource(R.string.nav_schedule)) },
                                    )
                                    NavigationRailItem(
                                        selected = tab == Tab.SETTINGS,
                                        onClick = { tab = Tab.SETTINGS },
                                        icon = { Icon(Icons.Filled.Settings, contentDescription = null) },
                                        label = { Text(stringResource(R.string.nav_settings)) },
                                    )
                                }
                            }
                            Box(Modifier.weight(1f).fillMaxHeight()) {
                                when (tab) {
                                    Tab.SCHEDULE -> ScheduleScreen(state, compactWidth, actions, banner = reminderBanner)
                                    Tab.SETTINGS -> SettingsScreen(
                                        state = state,
                                        onTheme = { viewModel.setTheme(it) },
                                        onReminders = { viewModel.setRemindersEnabled(it) },
                                        onImport = pickDocument,
                                        banner = reminderBanner,
                                    )
                                }
                            }
                            if (expanded && tab == Tab.SCHEDULE) {
                                DetailSidePane(
                                    state = state,
                                    onOpen = openLink,
                                    onToggleSkip = { occurrence, skipped -> viewModel.setSkipped(occurrence, skipped) },
                                    modifier = Modifier.width(DETAIL_PANE_WIDTH).fillMaxHeight(),
                                )
                            }
                        }
                    }

                    // Below the expanded width the detail slides up as a sheet instead.
                    val selected = state.selected
                    if (selected != null && !expanded && tab == Tab.SCHEDULE) {
                        ModalBottomSheet(onDismissRequest = viewModel::dismissDetail) {
                            LessonDetail(
                                occurrence = selected,
                                status = state.statuses[selected.lesson.id to selected.date],
                                onOpen = { openLink(selected) },
                                onToggleSkip = {
                                    val skipped = state.statuses[selected.lesson.id to selected.date] ==
                                        app.autopara.core.model.OccurrenceStatus.SKIPPED
                                    viewModel.setSkipped(selected, !skipped)
                                },
                            )
                        }
                    }
                }
            }
        }
    }
}

private val DETAIL_PANE_WIDTH = 360.dp

/** The permanent right-hand pane of a tablet: the open class, else the next one. */
@Composable
private fun DetailSidePane(
    state: UiState,
    onOpen: (Occurrence) -> Unit,
    onToggleSkip: (Occurrence, Boolean) -> Unit,
    modifier: Modifier = Modifier,
) {
    Surface(modifier, color = MaterialTheme.colorScheme.surface, tonalElevation = 1.dp) {
        val shown = state.selected ?: state.nextUp?.let { Occurrence(it.lesson, state.now.toLocalDate()) }
        if (shown == null) {
            Box(Modifier.fillMaxSize().padding(24.dp), contentAlignment = Alignment.Center) {
                Text(
                    stringResource(R.string.detail_nothing_selected),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    textAlign = TextAlign.Center,
                )
            }
        } else {
            val status = state.statuses[shown.lesson.id to shown.date]
            Column {
                if (state.selected == null) {
                    Text(
                        stringResource(R.string.detail_next_up),
                        style = MaterialTheme.typography.labelLarge,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(start = 20.dp, top = 20.dp),
                    )
                }
                LessonDetail(
                    occurrence = shown,
                    status = status,
                    onOpen = { onOpen(shown) },
                    onToggleSkip = { onToggleSkip(shown, status != app.autopara.core.model.OccurrenceStatus.SKIPPED) },
                )
            }
        }
    }
}

private fun messageText(context: Context, message: UiMessage): String = when (message) {
    is UiMessage.Imported -> {
        val s = message.summary
        if (s.withoutLink > 0) {
            context.getString(R.string.import_done_nolink, s.lessons, s.groups, s.withoutLink)
        } else {
            context.getString(R.string.import_done, s.lessons, s.groups)
        }
    }
    UiMessage.ImportFailed -> context.getString(R.string.import_failed)
    is UiMessage.AppMissing -> context.getString(R.string.app_missing_opened_browser)
    UiMessage.LinkFailed -> context.getString(R.string.link_could_not_open)
    UiMessage.NoLink -> context.getString(R.string.link_missing)
    UiMessage.NoMatch -> context.getString(R.string.no_match_found)
}
