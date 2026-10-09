package app.autopara.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.DateRange
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import app.autopara.R

/** First run: ask for the university's .docx timetable. */
@Composable
fun ImportScreen(importing: Boolean, onPick: () -> Unit, onStartEmpty: () -> Unit, modifier: Modifier = Modifier) {
    Box(modifier.fillMaxSize().padding(24.dp), contentAlignment = Alignment.Center) {
        Column(
            Modifier.widthIn(max = 460.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Box(
                Modifier.size(88.dp).background(MaterialTheme.colorScheme.primary, RoundedCornerShape(28.dp)),
                contentAlignment = Alignment.Center,
            ) {
                Icon(Icons.Filled.DateRange, contentDescription = null, tint = MaterialTheme.colorScheme.onPrimary, modifier = Modifier.size(44.dp))
            }
            Text(stringResource(R.string.import_title), style = MaterialTheme.typography.headlineSmall, textAlign = TextAlign.Center)
            Text(
                stringResource(R.string.import_body),
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.Center,
            )
            if (importing) {
                CircularProgressIndicator()
                Text(stringResource(R.string.importing), style = MaterialTheme.typography.bodyMedium)
            } else {
                Button(onClick = onPick) { Text(stringResource(R.string.import_button)) }
                androidx.compose.material3.TextButton(onClick = onStartEmpty) { Text(stringResource(R.string.start_empty)) }
            }
        }
    }
}
