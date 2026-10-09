package app.autopara.ui

import android.net.Uri
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.autopara.AppContainer
import app.autopara.core.importer.DocxReader
import app.autopara.core.model.Course
import app.autopara.core.model.Group
import app.autopara.core.model.Lesson
import app.autopara.core.model.Occurrence
import app.autopara.core.model.OccurrenceStatus
import app.autopara.core.model.Provider
import app.autopara.core.schedule.LessonDraft
import app.autopara.core.schedule.LessonFilter
import app.autopara.core.schedule.NextUp
import app.autopara.core.schedule.findMatch
import app.autopara.data.ImportSummary
import app.autopara.data.Settings
import app.autopara.data.ThemeMode
import app.autopara.data.ViewMode
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import java.io.IOException
import java.time.LocalDate
import java.time.LocalDateTime
import java.time.YearMonth

/** The class editor on screen: a new class ([lessonId] null) or an existing one being changed. */
data class EditorState(val lessonId: Long?, val draft: LessonDraft, val weekly: Boolean)

/** One-shot messages the screen turns into a snackbar. */
sealed interface UiMessage {
    data class Imported(val summary: ImportSummary) : UiMessage
    data object ImportFailed : UiMessage
    data class AppMissing(val provider: Provider) : UiMessage
    data object LinkFailed : UiMessage
    data object NoLink : UiMessage
    data object NoMatch : UiMessage
}

data class UiState(
    /** False until the database has answered once, so the import screen does not flash up. */
    val loaded: Boolean = false,
    val settings: Settings = Settings(),
    val courses: List<Course> = emptyList(),
    val groups: List<Group> = emptyList(),
    val selectedGroup: Group? = null,
    val lessons: List<Lesson> = emptyList(),
    val statuses: Map<Pair<Long, LocalDate>, OccurrenceStatus> = emptyMap(),
    val date: LocalDate = LocalDate.now(),
    val now: LocalDateTime = LocalDateTime.now(),
    val filter: LessonFilter = LessonFilter(),
    val subjects: List<String> = emptyList(),
    /** Lessons the filter lets through; the views draw these. */
    val visibleLessons: List<Lesson> = emptyList(),
    val nextUp: NextUp? = null,
    val selected: Occurrence? = null,
    val importing: Boolean = false,
    val message: UiMessage? = null,
    val editor: EditorState? = null,
) {
    val hasTimetable: Boolean get() = groups.isNotEmpty()
}

@OptIn(ExperimentalCoroutinesApi::class)
class MainViewModel(private val container: AppContainer) : ViewModel() {
    private val repository = container.repository
    private val settingsRepository = container.settings

    private val date = MutableStateFlow(LocalDate.now())
    private val filter = MutableStateFlow(LessonFilter())
    private val selection = MutableStateFlow<Pair<Long, LocalDate>?>(null)
    private val importing = MutableStateFlow(false)
    private val message = MutableStateFlow<UiMessage?>(null)
    private val editor = MutableStateFlow<EditorState?>(null)

    private val clock: Flow<LocalDateTime> = flow {
        while (true) {
            emit(LocalDateTime.now())
            delay(CLOCK_TICK_MS)
        }
    }

    private val lessons: Flow<List<Lesson>> = settingsRepository.settings
        .map { it.selectedGroupId }
        .distinctUntilChanged()
        .flatMapLatest { id -> if (id == null) flowOf(emptyList()) else repository.lessons(id) }

    /** Statuses for the month on screen plus a week either side (the month grid shows neighbours). */
    private val statuses = date
        .map { YearMonth.from(it) }
        .distinctUntilChanged()
        .flatMapLatest { month ->
            repository.statuses(month.atDay(1).minusDays(7), month.atEndOfMonth().plusDays(7))
        }

    private data class Core(val settings: Settings, val groups: List<Group>, val courses: List<Course>, val lessons: List<Lesson>)
    private data class Screen(val date: LocalDate, val filter: LessonFilter, val selection: Pair<Long, LocalDate>?, val now: LocalDateTime, val statuses: Map<Pair<Long, LocalDate>, OccurrenceStatus>)

    private val core = combine(settingsRepository.settings, repository.groups, repository.courses, lessons) { s, g, c, l ->
        Core(s, g, c, l)
    }
    private val screen = combine(date, filter, selection, clock, statuses) { d, f, sel, now, st ->
        Screen(d, f, sel, now, st)
    }
    private val skippedToday = repository.skipped(LocalDate.now()).map { set ->
        val today = LocalDate.now()
        set.filter { it.second == today }.map { it.first }.toSet()
    }

    private val overlay = combine(importing, message, editor) { busy, msg, ed -> Triple(busy, msg, ed) }

    val state: StateFlow<UiState> = combine(core, screen, skippedToday, overlay) { c, s, skipped, over ->
        val (busy, msg, ed) = over
        val group = c.groups.firstOrNull { it.id == c.settings.selectedGroupId } ?: c.groups.firstOrNull()
        val visible = c.lessons.filter { s.filter.matches(it) }
        val selected = s.selection?.let { (id, day) -> c.lessons.firstOrNull { it.id == id }?.takeIf { it.occursOn(day) }?.let { Occurrence(it, day) } }
        UiState(
            loaded = true,
            settings = c.settings,
            courses = c.courses,
            groups = c.groups,
            selectedGroup = group,
            lessons = c.lessons,
            statuses = s.statuses,
            date = s.date,
            now = s.now,
            filter = s.filter,
            subjects = LessonFilter.subjects(c.lessons),
            visibleLessons = visible,
            nextUp = NextUp.find(c.lessons, s.now, settled = skipped),
            selected = selected,
            importing = busy,
            message = msg,
            editor = ed,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), UiState())

    // ------------------------------------------------------------------ navigation

    fun selectDate(day: LocalDate) { date.value = day }

    fun goToday() { date.value = LocalDate.now() }

    /** Moves by one step of the current view: a day, a week or a month. */
    fun shift(direction: Int, mode: ViewMode) {
        date.update {
            when (mode) {
                ViewMode.DAY -> it.plusDays(direction.toLong())
                ViewMode.WEEK -> it.plusWeeks(direction.toLong())
                ViewMode.MONTH -> it.plusMonths(direction.toLong())
            }
        }
    }

    fun setViewMode(mode: ViewMode) = viewModelScope.launch { settingsRepository.setViewMode(mode) }

    // ------------------------------------------------------------------ search and filter

    fun setSearch(text: String) = filter.update { it.copy(text = text) }

    fun setSubject(subject: String) = filter.update { it.copy(subject = subject) }

    fun clearFilter() { filter.value = LessonFilter() }

    /** Jumps to the nearest day before/after the current one with a class that matches the filter. */
    fun jumpToMatch(direction: Int) {
        val current = state.value
        val found = findMatch(current.lessons, current.filter, current.date, direction)
        if (found != null) date.value = found else message.value = UiMessage.NoMatch
    }

    // ------------------------------------------------------------------ selection and status

    fun select(lessonId: Long, day: LocalDate) { selection.value = lessonId to day }

    fun dismissDetail() { selection.value = null }

    /** Opens a class from a tapped notification. */
    fun showFromNotification(lessonId: Long, day: LocalDate) {
        date.value = day
        selection.value = lessonId to day
    }

    fun setSkipped(occurrence: Occurrence, skipped: Boolean) {
        viewModelScope.launch {
            repository.setStatus(
                occurrence.lesson.id, occurrence.date,
                if (skipped) OccurrenceStatus.SKIPPED else null,
            )
        }
    }

    fun onLinkResult(occurrence: Occurrence, opened: Boolean) {
        if (opened) viewModelScope.launch { repository.markOpened(occurrence.lesson.id, occurrence.date) }
    }

    // ------------------------------------------------------------------ adding and editing classes

    /** Opens the editor on a blank class for [day] (the day on screen unless told otherwise). */
    fun startAdd(day: LocalDate = state.value.date) {
        editor.value = EditorState(null, LessonDraft.blank(day), weekly = false)
    }

    /** Opens the editor on an existing class, as it falls on [day]. */
    fun startEdit(lessonId: Long, day: LocalDate) {
        val lesson = state.value.lessons.firstOrNull { it.id == lessonId } ?: return
        val draft = LessonDraft.from(lesson, day)
        editor.value = EditorState(lessonId, draft, weekly = draft.weekly)
    }

    fun closeEditor() { editor.value = null }

    fun saveEditor(draft: LessonDraft) {
        val current = editor.value ?: return
        val groupId = state.value.selectedGroup?.id ?: return
        if (!draft.isValid) return
        viewModelScope.launch {
            val id = repository.saveLesson(current.lessonId, draft, groupId)
            editor.value = null
            date.value = draft.date
            selection.value = id to draft.date
        }
    }

    fun deleteLesson(lessonId: Long) {
        viewModelScope.launch {
            repository.deleteLesson(lessonId)
            editor.value = null
            selection.value = null
        }
    }

    /** First run, for someone who wants to type their classes in instead of importing a file. */
    fun startEmpty(name: String) {
        viewModelScope.launch { repository.createEmptyTimetable(name) }
    }

    // ------------------------------------------------------------------ settings

    fun selectGroup(id: Long) {
        viewModelScope.launch { settingsRepository.selectGroup(id) }
        selection.value = null
    }

    fun setTheme(mode: ThemeMode) = viewModelScope.launch { settingsRepository.setTheme(mode) }

    fun setRemindersEnabled(enabled: Boolean) = viewModelScope.launch { settingsRepository.setRemindersEnabled(enabled) }

    fun importFrom(uri: Uri) {
        viewModelScope.launch {
            importing.value = true
            try {
                val stream = container.context.contentResolver.openInputStream(uri) ?: throw IOException("cannot open $uri")
                val summary = stream.use { repository.importDocx(it) }
                selection.value = null
                message.value = UiMessage.Imported(summary)
            } catch (_: DocxReader.InvalidDocumentException) {
                message.value = UiMessage.ImportFailed
            } catch (_: IOException) {
                message.value = UiMessage.ImportFailed
            } finally {
                importing.value = false
            }
        }
    }

    fun showMessage(value: UiMessage) { message.value = value }

    fun consumeMessage() { message.value = null }

    private companion object {
        const val CLOCK_TICK_MS = 30_000L
    }
}
