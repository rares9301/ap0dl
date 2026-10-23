package com.ap0dl.anyomi.ui.theme

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

val Night = Color(0xFF0B0E14)
val NightRaised = Color(0xFF11161F)
val NightCard = Color(0xFF161C28)
val NightLine = Color(0xFF273044)
val Ice = Color(0xFFB0C6FF)
val IceDeep = Color(0xFF00429B)
val Mint = Color(0xFF7ADC77)
val Amber = Color(0xFFFFBD6B)
val Paper = Color(0xFFF1F3F9)
val Muted = Color(0xFFA6AFC2)

private val Colors = darkColorScheme(
    primary = Ice,
    onPrimary = Color(0xFF002D6E),
    primaryContainer = IceDeep,
    onPrimaryContainer = Color(0xFFD9E2FF),
    inversePrimary = Color(0xFF0058CA),
    secondary = Mint,
    onSecondary = Color(0xFF003909),
    secondaryContainer = Color(0xFF0B4A1C),
    onSecondaryContainer = Color(0xFF95F990),
    tertiary = Amber,
    onTertiary = Color(0xFF452B00),
    tertiaryContainer = Color(0xFF5B3A00),
    onTertiaryContainer = Color(0xFFFFDDB3),
    background = Night,
    onBackground = Paper,
    surface = NightRaised,
    onSurface = Paper,
    surfaceVariant = NightCard,
    onSurfaceVariant = Muted,
    surfaceContainer = NightRaised,
    surfaceContainerHigh = NightCard,
    surfaceContainerHighest = Color(0xFF20283A),
    surfaceContainerLow = Color(0xFF0E131C),
    surfaceContainerLowest = Color(0xFF080B10),
    outline = NightLine,
    outlineVariant = Color(0xFF1C2434),
    error = Color(0xFFFFB4AB),
    onError = Color(0xFF690005),
    errorContainer = Color(0xFF93000A),
    onErrorContainer = Color(0xFFFFDAD6),
    scrim = Color(0xCC000000),
)

private val Ap0dlTypography = Typography(
    displayLarge = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.Black,
        fontSize = 44.sp,
        lineHeight = 46.sp,
        letterSpacing = (-1.2).sp,
    ),
    displayMedium = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.Black,
        fontSize = 34.sp,
        lineHeight = 38.sp,
        letterSpacing = (-0.8).sp,
    ),
    headlineLarge = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.ExtraBold,
        fontSize = 30.sp,
        lineHeight = 34.sp,
        letterSpacing = (-0.5).sp,
    ),
    headlineMedium = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.ExtraBold,
        fontSize = 25.sp,
        lineHeight = 30.sp,
        letterSpacing = (-0.3).sp,
    ),
    headlineSmall = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.Bold,
        fontSize = 21.sp,
        lineHeight = 26.sp,
    ),
    titleLarge = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.Bold,
        fontSize = 19.sp,
        lineHeight = 24.sp,
    ),
    titleMedium = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.SemiBold,
        fontSize = 15.sp,
        lineHeight = 20.sp,
    ),
    titleSmall = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.SemiBold,
        fontSize = 13.sp,
        lineHeight = 17.sp,
        letterSpacing = 0.2.sp,
    ),
    bodyLarge = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.Normal,
        fontSize = 16.sp,
        lineHeight = 24.sp,
    ),
    bodyMedium = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.Normal,
        fontSize = 14.sp,
        lineHeight = 21.sp,
    ),
    bodySmall = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.Normal,
        fontSize = 12.sp,
        lineHeight = 17.sp,
        letterSpacing = 0.1.sp,
    ),
    labelLarge = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.Bold,
        fontSize = 14.sp,
        lineHeight = 18.sp,
        letterSpacing = 0.2.sp,
    ),
    labelMedium = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.SemiBold,
        fontSize = 11.sp,
        lineHeight = 15.sp,
        letterSpacing = 0.6.sp,
    ),
    labelSmall = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.Medium,
        fontSize = 10.sp,
        lineHeight = 13.sp,
        letterSpacing = 0.5.sp,
    ),
)

private val Ap0dlShapes = Shapes(
    extraSmall = RoundedCornerShape(8.dp),
    small = RoundedCornerShape(12.dp),
    medium = RoundedCornerShape(16.dp),
    large = RoundedCornerShape(24.dp),
    extraLarge = RoundedCornerShape(30.dp),
)

val HeroGradient = Brush.verticalGradient(
    colors = listOf(
        Color.Transparent,
        Color(0x990B0E14),
        Night,
    ),
)

val CardGradient = Brush.linearGradient(
    colors = listOf(
        Color(0xFF2A3C66),
        Color(0xFF172033),
    ),
)

@Composable
fun Ap0dlTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = Colors,
        typography = Ap0dlTypography,
        shapes = Ap0dlShapes,
        content = content,
    )
}