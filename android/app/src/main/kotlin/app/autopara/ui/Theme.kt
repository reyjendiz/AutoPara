package app.autopara.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import app.autopara.core.model.SubjectColor
import app.autopara.data.ThemeMode

/**
 * The desktop app's "Education Hub" look: white cards on a soft grey canvas, one near-black ink for
 * everything primary, and pastel colour only where it means something (a subject, a status).
 * The accent is ink rather than a hue, and the dark theme inverts the pair instead of inventing a
 * second accent. Values are the desktop tokens (`core/theme.py`).
 */
private val LightColors: ColorScheme = lightColorScheme(
    primary = Color(0xFF0E0E10),
    onPrimary = Color(0xFFFFFFFF),
    primaryContainer = Color(0xFFEFEFF1),
    onPrimaryContainer = Color(0xFF0E0E10),
    secondary = Color(0xFF6B6B75),
    onSecondary = Color(0xFFFFFFFF),
    secondaryContainer = Color(0xFFEFEFF1),
    onSecondaryContainer = Color(0xFF0E0E10),
    tertiary = Color(0xFF1F67AD),
    background = Color(0xFFF5F5F6),
    onBackground = Color(0xFF0E0E10),
    surface = Color(0xFFFFFFFF),
    onSurface = Color(0xFF0E0E10),
    surfaceVariant = Color(0xFFEFEFF1),
    onSurfaceVariant = Color(0xFF6B6B75),
    surfaceContainerLowest = Color(0xFFFFFFFF),
    surfaceContainerLow = Color(0xFFFAFAFB),
    surfaceContainer = Color(0xFFF5F5F6),
    surfaceContainerHigh = Color(0xFFEFEFF1),
    surfaceContainerHighest = Color(0xFFE6E6EA),
    outline = Color(0xFF7E7E88),
    outlineVariant = Color(0xFFE6E6EA),
    error = Color(0xFFC23A30),
    errorContainer = Color(0xFFFDECEA),
    onErrorContainer = Color(0xFFC23A30),
)

private val DarkColors: ColorScheme = darkColorScheme(
    primary = Color(0xFFF2F2F4),
    onPrimary = Color(0xFF0A0A0B),
    primaryContainer = Color(0xFF232326),
    onPrimaryContainer = Color(0xFFF2F2F4),
    secondary = Color(0xFFA0A0AA),
    onSecondary = Color(0xFF0A0A0B),
    secondaryContainer = Color(0xFF232326),
    onSecondaryContainer = Color(0xFFF2F2F4),
    tertiary = Color(0xFF86CDF7),
    background = Color(0xFF0A0A0B),
    onBackground = Color(0xFFF2F2F4),
    surface = Color(0xFF151517),
    onSurface = Color(0xFFF2F2F4),
    surfaceVariant = Color(0xFF1D1D20),
    onSurfaceVariant = Color(0xFFA0A0AA),
    surfaceContainerLowest = Color(0xFF0A0A0B),
    surfaceContainerLow = Color(0xFF121214),
    surfaceContainer = Color(0xFF151517),
    surfaceContainerHigh = Color(0xFF1D1D20),
    surfaceContainerHighest = Color(0xFF2A2A2E),
    outline = Color(0xFF8A8A94),
    outlineVariant = Color(0xFF2A2A2E),
    error = Color(0xFFE5534D),
    errorContainer = Color(0xFF3A1D1B),
    onErrorContainer = Color(0xFFFFB4AE),
)

private val AppTypography = Typography().let { base ->
    base.copy(
        headlineSmall = base.headlineSmall.copy(fontWeight = FontWeight.SemiBold),
        titleLarge = base.titleLarge.copy(fontWeight = FontWeight.SemiBold),
        titleMedium = base.titleMedium.copy(fontWeight = FontWeight.SemiBold),
        labelSmall = TextStyle(fontSize = 11.sp, lineHeight = 14.sp, fontWeight = FontWeight.Medium),
    )
}

/** Whether the app is drawing dark right now (the user's choice, else the system's). */
@Composable
fun isDark(mode: ThemeMode): Boolean = when (mode) {
    ThemeMode.SYSTEM -> isSystemInDarkTheme()
    ThemeMode.LIGHT -> false
    ThemeMode.DARK -> true
}

@Composable
fun AutoParaTheme(mode: ThemeMode, content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = if (isDark(mode)) DarkColors else LightColors,
        typography = AppTypography,
        content = content,
    )
}

/** The colour a subject wears: the same pastel as on the desktop. */
fun subjectColor(subject: String): Color = Color(0xFF000000.toInt() or SubjectColor.of(subject))

/** How strongly a card is washed with its subject colour (the desktop's TINT_LIGHT / TINT_DARK). */
fun tintAmount(dark: Boolean): Float = if (dark) 0.20f else 0.16f
