package app.aurora.player

import app.aurora.player.spike.WordBlurLaw
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Pure-math tests for the word-blur law shared by KaraokeLine and
 * AuroraLyricPreviewBox. These run on the JVM (no Android needed)
 * because [WordBlurLaw] has no Android dependencies.
 */
class WordBlurLawTest {

    @Test
    fun progressIsZeroBeforeTheWordAndOneAfterIt() {
        assertEquals(0f, WordBlurLaw.blurProgress(999, 1000, 2000), 0.001f)
        assertEquals(1f, WordBlurLaw.blurProgress(2000, 1000, 2000), 0.001f)
        assertEquals(1f, WordBlurLaw.blurProgress(5000, 1000, 2000), 0.001f)
    }

    @Test
    fun progressIsHalfwayInTheMiddleOfTheWindow() {
        assertEquals(0.5f, WordBlurLaw.blurProgress(1500, 1000, 2000), 0.001f)
    }

    @Test
    fun progressClampsOutsideTheWindow() {
        assertEquals(0f, WordBlurLaw.blurProgress(-100, 1000, 2000), 0.001f)
        assertEquals(1f, WordBlurLaw.blurProgress(99_999, 1000, 2000), 0.001f)
    }

    @Test
    fun zeroLengthWindowNeverProducesNaN() {
        assertEquals(0f, WordBlurLaw.blurProgress(999, 1000, 1000), 0.001f)
        assertEquals(1f, WordBlurLaw.blurProgress(1000, 1000, 1000), 0.001f)
        assertEquals(1f, WordBlurLaw.blurProgress(1001, 1000, 900), 0.001f)
    }

    @Test
    fun radiusShrinksFromMaxToZeroAsTheWordIsSung() {
        assertEquals(14f, WordBlurLaw.blurRadiusPx(0f), 0.001f)
        assertEquals(7f, WordBlurLaw.blurRadiusPx(0.5f), 0.001f)
        assertEquals(0f, WordBlurLaw.blurRadiusPx(1f), 0.001f)
        // Mini card uses the smaller maximum.
        assertEquals(8f, WordBlurLaw.blurRadiusPx(0f, WordBlurLaw.WORD_BLUR_MAX_PX), 0.001f)
        assertEquals(4f, WordBlurLaw.blurRadiusPx(0.5f, WordBlurLaw.WORD_BLUR_MAX_PX), 0.001f)
    }

    @Test
    fun alphaEqualsProgress() {
        assertEquals(0f, WordBlurLaw.alpha(0f), 0.001f)
        assertEquals(0.25f, WordBlurLaw.alpha(0.25f), 0.001f)
        assertEquals(1f, WordBlurLaw.alpha(1f), 0.001f)
    }

    @Test
    fun strokePassesMatchThePhaseOneSpec() {
        // 4 passes, widths 5 → 3.5 → 2 → 0.8, alphas 0.035 → 0.17.
        assertEquals(4, WordBlurLaw.STROKE_PASSES.size)
        assertEquals(5f, WordBlurLaw.STROKE_PASSES[0].first, 0.001f)
        assertEquals(0.035f, WordBlurLaw.STROKE_PASSES[0].second, 0.001f)
        assertEquals(3.5f, WordBlurLaw.STROKE_PASSES[1].first, 0.001f)
        assertEquals(0.06f, WordBlurLaw.STROKE_PASSES[1].second, 0.001f)
        assertEquals(2f, WordBlurLaw.STROKE_PASSES[2].first, 0.001f)
        assertEquals(0.10f, WordBlurLaw.STROKE_PASSES[2].second, 0.001f)
        assertEquals(0.8f, WordBlurLaw.STROKE_PASSES[3].first, 0.001f)
        assertEquals(0.17f, WordBlurLaw.STROKE_PASSES[3].second, 0.001f)
        // Widths narrow monotonically; alphas grow monotonically.
        for (i in 1 until WordBlurLaw.STROKE_PASSES.size) {
            assertTrue(WordBlurLaw.STROKE_PASSES[i].first < WordBlurLaw.STROKE_PASSES[i - 1].first)
            assertTrue(WordBlurLaw.STROKE_PASSES[i].second > WordBlurLaw.STROKE_PASSES[i - 1].second)
        }
    }

    @Test
    fun lyricWordBlurProgressUsesItsOwnWindow() {
        val word = LyricWord(text = "hello", activation = 0f, startMs = 1000, endMs = 2000)
        assertEquals(0f, word.blurProgress(1000), 0.001f)
        assertEquals(0.5f, word.blurProgress(1500), 0.001f)
        assertEquals(1f, word.blurProgress(2000), 0.001f)
    }

    @Test
    fun lyricWordWithoutWindowFallsBackToActivation() {
        val word = LyricWord(text = "hello", activation = 0.4f)
        assertEquals(0.4f, word.blurProgress(12345), 0.001f)
    }

    @Test
    fun rtlDetectionUsesFirstStrongCharacter() {
        assertEquals(true, isRtlText("سَلامٌ على الدُّنيا"))
        assertEquals(true, isRtlText("  123 سلام"))
        assertEquals(false, isRtlText("Hello Aurora"))
        assertEquals(false, isRtlText("123 Hello سلام"))
        assertEquals(false, isRtlText(""))
        assertEquals(false, isRtlText(null))
        assertEquals(false, isRtlText("  ...  "))
    }
}
