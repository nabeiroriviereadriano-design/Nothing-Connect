package io.github.nabeiroriviereadriano.nothingconnect.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

object Nc {
    val Black = Color(0xFF000000)
    val White = Color(0xFFFFFFFF)
    val Surface = Color(0xFF161616)
    val Surface2 = Color(0xFF1F1F1F)
    val Line = Color(0xFF2E2E2E)
    val Muted = Color(0xFF8E8E8E)
    val Red = Color(0xFFD71921)
}

@Composable
fun NcTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = darkColorScheme(
            primary = Nc.White, onPrimary = Nc.Black,
            secondary = Nc.White, onSecondary = Nc.Black,
            background = Nc.Black, onBackground = Nc.White,
            surface = Nc.Surface, onSurface = Nc.White,
            surfaceVariant = Nc.Surface2, onSurfaceVariant = Nc.Muted,
            surfaceContainer = Nc.Surface, surfaceContainerHigh = Nc.Surface2, surfaceContainerHighest = Nc.Surface2,
            outline = Nc.Line, error = Nc.Red,
        ),
        typography = Typography(
            headlineMedium = TextStyle(fontSize = 28.sp, fontWeight = FontWeight.Medium, lineHeight = 32.sp),
            titleMedium = TextStyle(fontSize = 18.sp, fontWeight = FontWeight.Medium),
            bodyLarge = TextStyle(fontSize = 16.sp, lineHeight = 23.sp),
            bodyMedium = TextStyle(fontSize = 14.sp, lineHeight = 20.sp),
        ),
        content = content,
    )
}

/** Glyphes 5×7 pour l'affichage en matrice de points (chiffres et quelques signes). */
private val GLYPHS = mapOf(
    '0' to listOf("01110", "10001", "10011", "10101", "11001", "10001", "01110"),
    '1' to listOf("00100", "01100", "00100", "00100", "00100", "00100", "01110"),
    '2' to listOf("01110", "10001", "00001", "00010", "00100", "01000", "11111"),
    '3' to listOf("11110", "00001", "00001", "01110", "00001", "00001", "11110"),
    '4' to listOf("00010", "00110", "01010", "10010", "11111", "00010", "00010"),
    '5' to listOf("11111", "10000", "11110", "00001", "00001", "10001", "01110"),
    '6' to listOf("00110", "01000", "10000", "11110", "10001", "10001", "01110"),
    '7' to listOf("11111", "00001", "00010", "00100", "01000", "01000", "01000"),
    '8' to listOf("01110", "10001", "10001", "01110", "10001", "10001", "01110"),
    '9' to listOf("01110", "10001", "10001", "01111", "00001", "00010", "01100"),
    '-' to listOf("00000", "00000", "00000", "11111", "00000", "00000", "00000"),
    '%' to listOf("11001", "11010", "00010", "00100", "01000", "01011", "10011"),
    ' ' to List(7) { "00000" },
)

/** Texte en matrice de points dessiné point par point (identité visuelle Nothing). */
@Composable
fun DotText(text: String, dot: Dp = 5.dp, color: Color = Nc.White, modifier: Modifier = Modifier) {
    val chars = text.map { GLYPHS[it] ?: GLYPHS[' ']!! }
    val pitch = dot * 1.6f
    val width = pitch * (chars.size * 6 - 1)
    Canvas(modifier.size(width, pitch * 7).semantics { contentDescription = text }) {
        val p = pitch.toPx()
        val r = dot.toPx() / 2
        chars.forEachIndexed { ci, g ->
            g.forEachIndexed { y, row ->
                row.forEachIndexed { x, c ->
                    val center = Offset((ci * 6 + x) * p + p / 2, y * p + p / 2)
                    if (c == '1') drawCircle(color, r, center) else drawCircle(Nc.Line, r * 0.35f, center)
                }
            }
        }
    }
}
