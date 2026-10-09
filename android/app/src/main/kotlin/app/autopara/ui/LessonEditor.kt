package app.autopara.ui

import android.app.DatePickerDialog
import android.app.TimePickerDialog
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import app.autopara.R
import app.autopara.core.schedule.LessonDraft
import java.time.LocalDate
import java.time.LocalTime

/**
 * Creates or edits a class. The fields live in `rememberSaveable` state, so a half-typed class
 * survives a rotation; nothing is written until Save, and Save is refused (with the reason shown
 * next to the field) while the class is not valid.
 *
 * @param editing true when changing an existing class: shows Delete and the "applies to every week" note.
 * @param shared true when the class belongs to several groups.
 */
@Composable
fun LessonEditorDialog(
    initial: LessonDraft,
    editing: Boolean,
    shared: Boolean,
    onSave: (LessonDraft) -> Unit,
    onDelete: () -> Unit,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current
    val locale = currentLocale()

    var subject by rememberSaveable { mutableStateOf(initial.subject) }
    var teacher by rememberSaveable { mutableStateOf(initial.teacher) }
    var url by rememberSaveable { mutableStateOf(initial.url) }
    var epochDay by rememberSaveable { mutableLongStateOf(initial.date.toEpochDay()) }
    var weekly by rememberSaveable { mutableStateOf(initial.weekly) }
    var startMinute by rememberSaveable { mutableIntStateOf(initial.start.toSecondOfDay() / 60) }
    var endMinute by rememberSaveable { mutableIntStateOf(initial.end.toSecondOfDay() / 60) }
    var tried by rememberSaveable { mutableStateOf(false) }
    var confirmDelete by rememberSaveable { mutableStateOf(false) }

    fun draft() = LessonDraft(
        subject = subject, teacher = teacher, url = url,
        date = LocalDate.ofEpochDay(epochDay), weekly = weekly,
        start = LocalTime.of(startMinute / 60, startMinute % 60),
        end = LocalTime.of(endMinute / 60, endMinute % 60),
    )

    val problems = if (tried) draft().problems() else emptyList()

    AlertDialog(
        onDismissRequest = onDismiss,
        title = {
            Text(stringResource(if (editing) R.string.editor_title_edit else R.string.editor_title_add))
        },
        text = {
            Column(
                Modifier.verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                OutlinedTextField(
                    value = subject,
                    onValueChange = { subject = it },
                    label = { Text(stringResource(R.string.field_subject)) },
                    isError = LessonDraft.Problem.EMPTY_SUBJECT in problems,
                    supportingText = {
                        if (LessonDraft.Problem.EMPTY_SUBJECT in problems) Text(stringResource(R.string.error_subject))
                    },
                    modifier = Modifier.fillMaxWidth(),
                )
                OutlinedTextField(
                    value = teacher,
                    onValueChange = { teacher = it },
                    label = { Text(stringResource(R.string.field_teacher)) },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
                OutlinedTextField(
                    value = url,
                    onValueChange = { url = it },
                    label = { Text(stringResource(R.string.field_link)) },
                    singleLine = true,
                    isError = LessonDraft.Problem.BAD_LINK in problems,
                    supportingText = {
                        if (LessonDraft.Problem.BAD_LINK in problems) Text(stringResource(R.string.error_link))
                    },
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
                    modifier = Modifier.fillMaxWidth(),
                )

                // Date: the system date picker. A weekly class only keeps this date's weekday.
                OutlinedButton(
                    onClick = {
                        val day = LocalDate.ofEpochDay(epochDay)
                        DatePickerDialog(
                            context,
                            { _, year, month, dayOfMonth -> epochDay = LocalDate.of(year, month + 1, dayOfMonth).toEpochDay() },
                            day.year, day.monthValue - 1, day.dayOfMonth,
                        ).show()
                    },
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Text("${stringResource(R.string.field_date)}: ${dayTitle(LocalDate.ofEpochDay(epochDay), locale)}")
                }

                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    TimeButton(stringResource(R.string.field_from), startMinute, Modifier.weight(1f)) { picked ->
                        startMinute = picked
                        if (endMinute <= picked) endMinute = minOf(picked + LessonDraft.DEFAULT_MINUTES.toInt(), 23 * 60 + 59)
                    }
                    TimeButton(stringResource(R.string.field_to), endMinute, Modifier.weight(1f)) { endMinute = it }
                }
                if (LessonDraft.Problem.END_NOT_AFTER_START in problems) {
                    Text(
                        stringResource(R.string.error_time),
                        color = MaterialTheme.colorScheme.error,
                        style = MaterialTheme.typography.bodySmall,
                    )
                }

                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(stringResource(R.string.repeat_weekly), modifier = Modifier.weight(1f))
                    Switch(checked = weekly, onCheckedChange = { weekly = it })
                }
                if (editing && weekly) {
                    Text(
                        stringResource(R.string.editor_applies_all),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                if (editing && shared) {
                    Text(
                        stringResource(R.string.editor_applies_shared),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                if (editing) {
                    TextButton(onClick = { confirmDelete = true }) {
                        Text(stringResource(R.string.delete), color = MaterialTheme.colorScheme.error)
                    }
                }
            }
        },
        confirmButton = {
            TextButton(onClick = {
                tried = true
                val ready = draft()
                if (ready.isValid) onSave(ready)
            }) { Text(stringResource(R.string.save)) }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) }
        },
    )

    if (confirmDelete) {
        AlertDialog(
            onDismissRequest = { confirmDelete = false },
            title = { Text(stringResource(R.string.delete_title)) },
            text = {
                Text(stringResource(if (weekly) R.string.delete_body_weekly else R.string.delete_body_once))
            },
            confirmButton = {
                TextButton(onClick = {
                    confirmDelete = false
                    onDelete()
                }) { Text(stringResource(R.string.delete), color = MaterialTheme.colorScheme.error) }
            },
            dismissButton = {
                TextButton(onClick = { confirmDelete = false }) { Text(stringResource(R.string.cancel)) }
            },
        )
    }
}

/** A button showing "From 08:00" that opens the system time picker (24-hour). */
@Composable
private fun TimeButton(label: String, minute: Int, modifier: Modifier, onPicked: (Int) -> Unit) {
    val context = LocalContext.current
    OutlinedButton(
        onClick = {
            TimePickerDialog(context, { _, hour, min -> onPicked(hour * 60 + min) }, minute / 60, minute % 60, true).show()
        },
        modifier = modifier,
    ) {
        Text("$label ${LocalTime.of(minute / 60, minute % 60).clock()}")
    }
}
