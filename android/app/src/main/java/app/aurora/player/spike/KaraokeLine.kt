package app.aurora.player.spike

import android.graphics.Paint
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.ExperimentalTextApi
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.aurora.player.LyricWord

/**
 * Word-level blur law — Phase 1 of the word-blur rebuild.
 *
 * This file recreates, from the handoff prompt, the effect that previously
 * lived in `app.aurora.player.spike.KaraokeLine` in the lost workspace:
 *
 * - `BLUR_MAX_PX = 14f` for the full lyric line.
 * - `drawBlurredWord()` draws 4 stroke passes whose widths narrow
 *   5 → 3.5 → 2 → 0.8 (px at density 1.0) with alphas
 *   0.035, 0.06, 0.10, 0.17. Overlapping wide, faint strokes spread the
 *   ink outward, which reads as blur without a per-frame RenderEffect
 *   (the Phase 0 measurement showed RenderEffect blur janks on Mali).
 * - The law, for one word with singing window `[ws, we)` at audio-clock
 *   position `nowMs`:
 *
 *       blurProgress = clamp((nowMs - ws) / (we - ws), 0, 1)
 *       blurRadius   = BLUR_MAX_PX * (1 - blurProgress)
 *       alpha        = blurProgress            (0 → 1)
 *
 *   So a word starts fully blurred and faint, and becomes sharp and
 *   opaque exactly as it is sung. It is a pure function of the audio
 *   clock position — no UI timer — so pause/seek stay correct.
 *
 * Liquid-glass / Phase D design-system work was cancelled by the user, so
 * this lives in the app module (package `spike`, matching the original
 * path) instead of a separate `:designsystem` module. The mini-card
 * variant is `AuroraLyricPreviewBox` in `designsystem/Components.kt`,
 * which reuses this exact law with a smaller maximum.
 */
object WordBlurLaw {
    /** Maximum blur radius for the full lyric line, px at density 1.0. */
    const val BLUR_MAX_PX = 14f

    /** Maximum blur radius for the mini lyric preview card, px at density 1.0.
     *  Smaller than [BLUR_MAX_PX] because the card's type is smaller. */
    const val WORD_BLUR_MAX_PX = 8f

    /** Stroke passes: (stroke width px at density 1.0, alpha). */
    val STROKE_PASSES: List<Pair<Float, Float>> = listOf(
        5f to 0.035f,
        3.5f to 0.06f,
        2f to 0.10f,
        0.8f to 0.17f,
    )

    /**
     * `blurProgress` for a word window at [nowMs], clamped to `0..1`.
     * A zero-length window (legacy payload) yields 0 before the word and
     * 1 at/after it, never NaN.
     */
    fun blurProgress(nowMs: Long, wordStartMs: Long, wordEndMs: Long): Float {
        if (wordEndMs <= wordStartMs) {
            return if (nowMs >= wordStartMs) 1f else 0f
        }
        return ((nowMs - wordStartMs).toFloat() / (wordEndMs - wordStartMs).toFloat())
            .coerceIn(0f, 1f)
    }

    /** `blurRadius = maxPx * (1 - progress)`, px at density 1.0. */
    fun blurRadiusPx(progress: Float, maxPx: Float = BLUR_MAX_PX): Float =
        maxPx * (1f - progress.coerceIn(0f, 1f))

    /** Fill alpha `0..1`: the word is invisible-sharp at 0, fully sharp at 1. */
    fun alpha(progress: Float): Float = progress.coerceIn(0f, 1f)
}

/**
 * Draws one word with the word-blur law on the current [androidx.compose.ui.graphics.Canvas]
 * draw scope, at baseline ([x], [baselineY]) in px.
 *
 * [density] scales the px constants so the effect looks the same on
 * mdpi and xxxhdpi screens. [paint] is reused across passes/words.
 */
fun androidx.compose.ui.graphics.drawscope.DrawScope.drawBlurredWord(
    text: String,
    x: Float,
    baselineY: Float,
    blurProgress: Float,
    color: Color,
    maxBlurPx: Float,
    density: Float,
    paint: Paint,
) {
    val progress = blurProgress.coerceIn(0f, 1f)
    val radius = WordBlurLaw.blurRadiusPx(progress, maxBlurPx)
    val scale = if (maxBlurPx > 0f) radius / maxBlurPx else 0f
    val argb = color.toArgb()
    val canvas = drawContext.canvas.nativeCanvas

    if (progress >= 0.999f || scale <= 0.001f) {
        // Fully sung: one sharp fill, no halo passes.
        paint.style = Paint.Style.FILL
        paint.strokeWidth = 0f
        paint.color = argb
        paint.alpha = 255
        canvas.drawText(text, x, baselineY, paint)
        return
    }

    // Blurred halo: 4 narrowing stroke passes. Widths shrink with the
    // radius, so the spread collapses as the word becomes sharp.
    paint.style = Paint.Style.STROKE
    paint.color = argb
    for ((widthPx, passAlpha) in WordBlurLaw.STROKE_PASSES) {
        paint.strokeWidth = (widthPx * scale * density).coerceAtLeast(0.5f)
        paint.alpha = (passAlpha * 255f).toInt().coerceIn(0, 255)
        canvas.drawText(text, x, baselineY, paint)
    }

    // Sharp core fading in with the progress.
    paint.style = Paint.Style.FILL
    paint.strokeWidth = 0f
    paint.alpha = (WordBlurLaw.alpha(progress) * 255f).toInt().coerceIn(0, 255)
    canvas.drawText(text, x, baselineY, paint)
}

/**
 * One lyric line rendered word-by-word with the word-blur effect.
 *
 * Words are measured with Compose's [androidx.compose.ui.text.TextMeasurer]
 * and drawn on a Canvas, one [drawBlurredWord] per word, so each word's
 * blur follows its own singing window. Arabic joining is preserved:
 * words are split only on spaces (Arabic never joins across a space),
 * and each word is drawn as a whole string, never letter-by-letter.
 *
 * [isRtl] lays the line out right-to-left (first word rightmost), which
 * is how the paragraph direction of an Arabic/mixed line reads. Media
 * controls elsewhere in the app stay LTR; this is lyric text only.
 *
 * [nowMs] must be the audio-clock position (Rust `positionMs`), updated
 * by the 200 ms poll in `PlayerViewModel`. Between polls the blur steps
 * at poll rate — smooth per-frame interpolation is a later optimisation
 * (see BLUR_REBUILD.md), not faked here.
 */
@OptIn(ExperimentalTextApi::class)
@Composable
fun KaraokeLine(
    words: List<LyricWord>,
    nowMs: Long,
    isRtl: Boolean,
    fontSize: TextUnit = 20.sp,
    color: Color = Color.White,
    maxBlurPx: Float = WordBlurLaw.BLUR_MAX_PX,
    fontWeight: FontWeight = FontWeight.SemiBold,
    modifier: Modifier = Modifier,
) {
    val density = LocalDensity.current
    val textMeasurer = rememberTextMeasurer()
    val style = remember(fontSize, fontWeight, color) {
        TextStyle(fontSize = fontSize, fontWeight = fontWeight, color = color)
    }
    val paint = remember { Paint().apply { isAntiAlias = true } }
    // TextUnit.value is the sp amount; using it as dp keeps the line
    // height proportional to the font size without a Density conversion.
    val heightDp = (fontSize.value * 1.5f).dp

    Canvas(modifier = modifier.fillMaxWidth().height(heightDp)) {
        if (words.isEmpty()) return@Canvas

        val spaceWidth = textMeasurer.measureText(" ", style).size.width.toFloat()
        val measured = words.map { word ->
            word to textMeasurer.measureText(word.text, style).size.width.toFloat()
        }
        val totalWidth = measured.sumOf { it.second.toDouble() }.toFloat() +
            spaceWidth * (measured.size - 1).coerceAtLeast(0)

        val textSizePx = with(density) { fontSize.toPx() }
        paint.textSize = textSizePx
        paint.typeface = android.graphics.Typeface.DEFAULT_BOLD
        val fm = paint.fontMetrics
        val lineHeightPx = fm.descent - fm.ascent
        val baselineY = (size.height - lineHeightPx) / 2f - fm.ascent

        var cursorX = if (isRtl) {
            (size.width + totalWidth) / 2f
        } else {
            (size.width - totalWidth) / 2f
        }
        val direction = if (isRtl) -1f else 1f

        for ((word, wordWidth) in measured) {
            val drawX = if (isRtl) cursorX - wordWidth else cursorX
            drawBlurredWord(
                text = word.text,
                x = drawX,
                baselineY = baselineY,
                blurProgress = word.blurProgress(nowMs),
                color = color,
                maxBlurPx = maxBlurPx,
                density = density.density,
                paint = paint,
            )
            cursorX += direction * (wordWidth + spaceWidth)
        }
    }
}
