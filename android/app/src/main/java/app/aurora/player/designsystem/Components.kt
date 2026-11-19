package app.aurora.player.designsystem

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.aurora.player.LyricLine
import app.aurora.player.Lyrics
import app.aurora.player.spike.KaraokeLine
import app.aurora.player.spike.WordBlurLaw

/**
 * Mini lyric preview card — Phase 2 of the word-blur rebuild.
 *
 * The small glass-less card shown under the transport controls (liquid
 * glass was cancelled, so this is a plain dark surface card): it shows
 * the current line with the same word-level blur law as the full lyric
 * line ([KaraokeLine] / [WordBlurLaw]), and the next line dimmed below
 * it. Tapping the card opens the full lyrics (wired by the caller via
 * [onClick]); the queue affordance stays with the caller, as before.
 *
 * Differences from the full line, by design:
 * - `WORD_BLUR_MAX_PX = 8f` instead of `BLUR_MAX_PX = 14f`, because the
 *   card's type is smaller — the same law, a smaller maximum.
 * - `blurProgress = (nowMs - ws) / (we - ws)` per word, computed in
 *   [app.aurora.player.LyricWord.blurProgress] from the Rust word
 *   windows and the audio-clock `positionMs`.
 * - RTL: [isRtl] comes from the paragraph direction of the lyric text
 *   (see `isRtlText` in MainActivity), not from the device locale, so an
 *   Arabic song in an English UI still lays out right-to-left.
 *
 * This component lives in the app module under a `designsystem` package
 * because the rebuilt codebase has no `:designsystem` Gradle module
 * (Phase D was cancelled). It is self-contained and can be moved into a
 * real design-system module later without API changes.
 */
@Composable
fun AuroraLyricPreviewBox(
    currentLine: LyricLine?,
    nextLine: LyricLine?,
    nowMs: Long,
    isRtl: Boolean,
    modifier: Modifier = Modifier,
    onClick: (() -> Unit)? = null,
) {
    val shape = RoundedCornerShape(20.dp)
    val cardModifier = modifier
        .fillMaxWidth()
        .clip(shape)
        .background(MaterialTheme.colorScheme.surface)
        .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
        .padding(horizontal = 16.dp, vertical = 12.dp)

    Column(cardModifier, verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                text = "LYRICS",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(end = 8.dp),
            )
            Column(Modifier.weight(1f)) {
                when {
                    currentLine == null && nextLine == null -> {
                        Text(
                            text = "No lyrics",
                            fontSize = 16.sp,
                            fontWeight = FontWeight.SemiBold,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                    currentLine != null -> {
                        KaraokeLine(
                            words = currentLine.words.ifEmpty {
                                listOf(
                                    app.aurora.player.LyricWord(
                                        text = currentLine.text,
                                        activation = currentLine.activation,
                                        startMs = currentLine.startMs,
                                        endMs = currentLine.endMs,
                                    )
                                )
                            },
                            nowMs = nowMs,
                            isRtl = isRtl,
                            fontSize = 18.sp,
                            color = MaterialTheme.colorScheme.onSurface,
                            maxBlurPx = WordBlurLaw.WORD_BLUR_MAX_PX,
                        )
                        nextLine?.let { next ->
                            Text(
                                text = next.text,
                                fontSize = 14.sp,
                                color = MaterialTheme.colorScheme.onSurfaceVariant
                                    .copy(alpha = 0.4f),
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                    }
                    else -> {
                        // No active line yet (before the first line): show the
                        // upcoming line dimmed instead of an empty card.
                        Text(
                            text = nextLine?.text.orEmpty(),
                            fontSize = 16.sp,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                                .copy(alpha = 0.6f),
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
            }
        }
    }
}

/**
 * Convenience overload taking the whole [Lyrics] payload: picks the
 * active line and the following line, and uses its audio-clock
 * [Lyrics.positionMs] as `nowMs`.
 */
@Composable
fun AuroraLyricPreviewBox(
    lyrics: Lyrics?,
    isRtl: Boolean,
    modifier: Modifier = Modifier,
    onClick: (() -> Unit)? = null,
) {
    val activeIndex = lyrics?.activeLine
    val current = activeIndex?.let { lyrics.lines.getOrNull(it) }
    val next = activeIndex?.let { lyrics.lines.getOrNull(it + 1) }
        ?: lyrics?.lines?.firstOrNull()
    AuroraLyricPreviewBox(
        currentLine = current,
        nextLine = next,
        nowMs = lyrics?.positionMs ?: 0L,
        isRtl = isRtl,
        modifier = modifier,
        onClick = onClick,
    )
}
